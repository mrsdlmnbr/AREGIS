//! resolver — Sighting → Track → Entity → Identity (spec §9.3).
//!
//! M0 scope: deterministic association good enough to fuse the synthetic
//! estate's sensors into single entities, mint durable unknown handles, bind
//! enrolled identities, and stamp expectations. The full Mahalanobis/
//! Hungarian/EKF pipeline is M2; the *shape* of the output — and the rule
//! that ambiguity is carried, never silently resolved — is fixed now.

use aegis_common::clock::Timestamp;
use aegis_common::geometry::{Point, Polygon};
use aegis_common::hash::sha256;
use aegis_common::types::{ObjectClass, ZoneClass};
use std::collections::BTreeSet;

/// Same-class detections within this many metres and seconds associate to
/// one entity. Generous for the synthetic estate's coordinate scale; tuned
/// against the golden corpus in M2, never against a live estate.
const ASSOC_GATE_METRES: f64 = 120.0;
const ASSOC_GATE_SECONDS: i64 = 6;
/// A motion sensor firing in the same zone within this window corroborates.
const MOTION_CORROBORATION_SECONDS: i64 = 10;
/// Below this speed the entity is dwelling.
const DWELL_SPEED_MPS: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct ZoneDef {
    pub id: String,
    pub class: ZoneClass,
    pub area: Polygon,
}

#[derive(Debug, Clone)]
pub struct ExpectationDef {
    pub id: String,
    pub person_id: Option<String>,
    pub vehicle_id: Option<String>,
    pub zone_ids: Vec<String>,
    pub start: Timestamp,
    pub end: Timestamp,
}

#[derive(Debug, Clone)]
pub struct SightingIn {
    pub id: String,
    pub event_id: String,
    pub device_id: String,
    pub at: Timestamp,
    pub class: ObjectClass,
    pub confidence: f64,
    pub world_position: Point,
    /// Sim shorthand for a gallery match: the enrolled identity this
    /// sighting's embedding resolves to. Real pipeline: cosine similarity
    /// over pgvector against consented, enrolment-only templates (§3.5).
    pub embedding_of: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: String,
    pub class: ObjectClass,
    /// "person:ana" | "vehicle:8XJ-4410" | "unknown:7F3A"
    pub identity_id: String,
    pub identity_known: bool,
    pub position: Point,
    pub trajectory: Vec<Point>,
    pub last_at: Timestamp,
    pub confidence: f64,
    pub zone_id: String,
    pub zone_class: ZoneClass,
    pub dwell_seconds: f64,
    pub expected: bool,
    pub expectation_id: Option<String>,
    pub devices: BTreeSet<String>,
    pub contributing_event_ids: Vec<String>,
    miss_product: f64,
}

pub struct Resolver {
    property_id: String,
    zones: Vec<ZoneDef>,
    expectations: Vec<ExpectationDef>,
    pub entities: Vec<Entity>,
    motion_events: Vec<(String, String, Timestamp)>, // device, zone, at
    entity_seq: u64,
}

impl Resolver {
    pub fn new(property_id: &str, zones: Vec<ZoneDef>, expectations: Vec<ExpectationDef>) -> Self {
        Self {
            property_id: property_id.to_string(),
            zones,
            expectations,
            entities: Vec::new(),
            motion_events: Vec::new(),
            entity_seq: 0,
        }
    }

    /// Zone lookup: first match wins (fixture order = most specific first).
    /// A position in no zone maps to the property's implicit GROUNDS.
    pub fn zone_at(&self, p: &Point) -> (String, ZoneClass) {
        for z in &self.zones {
            if z.area.contains(p) {
                return (z.id.clone(), z.class);
            }
        }
        ("offsite".to_string(), ZoneClass::Perimeter)
    }

    pub fn ingest_motion(&mut self, device_id: &str, zone_id: &str, at: Timestamp) {
        self.motion_events.push((device_id.to_string(), zone_id.to_string(), at));
    }

    /// Ingest one sighting; associate or mint. Returns the entity id.
    pub fn ingest_sighting(&mut self, s: SightingIn) -> String {
        // Association pass: gallery identity beats geometry; then class +
        // space-time gate.
        let matched = self
            .entities
            .iter_mut()
            .filter(|e| e.class == s.class)
            .find(|e| {
                if let Some(known) = &s.embedding_of {
                    if e.identity_id == *known {
                        return true;
                    }
                }
                let dt = (s.at - e.last_at).num_seconds();
                dt >= 0
                    && dt <= ASSOC_GATE_SECONDS
                    && e.position.dist(&s.world_position) <= ASSOC_GATE_METRES
            });

        let (zone_id, zone_class) = {
            // borrow dance: compute zone before mutable update
            let mut found = ("offsite".to_string(), ZoneClass::Perimeter);
            for z in &self.zones {
                if z.area.contains(&s.world_position) {
                    found = (z.id.clone(), z.class);
                    break;
                }
            }
            found
        };

        let entity_id = if let Some(e) = matched {
            let dt = (s.at - e.last_at).num_milliseconds() as f64 / 1000.0;
            let dist = e.position.dist(&s.world_position);
            if dt > 0.0 && dist / dt < DWELL_SPEED_MPS {
                e.dwell_seconds += dt;
            } else {
                e.dwell_seconds = 0.0;
            }
            e.position = s.world_position;
            e.trajectory.push(s.world_position);
            e.last_at = s.at;
            e.zone_id = zone_id;
            e.zone_class = zone_class;
            e.devices.insert(s.device_id.clone());
            e.contributing_event_ids.push(s.event_id.clone());
            // Independent-miss fusion: three mediocre sensors beat any one.
            e.miss_product *= 1.0 - s.confidence;
            e.confidence = (1.0 - e.miss_product).min(0.999);
            if let Some(known) = &s.embedding_of {
                e.identity_id = known.clone();
                e.identity_known = true;
            }
            e.id.clone()
        } else {
            self.entity_seq += 1;
            let (identity_id, identity_known) = match &s.embedding_of {
                Some(known) => (known.clone(), true),
                None => (self.mint_unknown_handle(&s), false),
            };
            let e = Entity {
                id: format!("ent-{:06}", self.entity_seq),
                class: s.class,
                identity_id,
                identity_known,
                position: s.world_position,
                trajectory: vec![s.world_position],
                last_at: s.at,
                confidence: s.confidence,
                zone_id,
                zone_class,
                dwell_seconds: 0.0,
                expected: false,
                expectation_id: None,
                devices: BTreeSet::from([s.device_id.clone()]),
                contributing_event_ids: vec![s.event_id.clone()],
                miss_product: 1.0 - s.confidence,
            };
            let id = e.id.clone();
            self.entities.push(e);
            id
        };

        self.stamp_expectation(&entity_id, s.at);
        entity_id
    }

    /// Sim-only ground-truth movement (scripted `entity_reaches` / FLEE).
    /// The real system only ever learns positions from sightings.
    pub fn advance_entity(&mut self, entity_id: &str, pos: Point, at: Timestamp) {
        let (zone_id, zone_class) = self.zone_at(&pos);
        if let Some(e) = self.entities.iter_mut().find(|e| e.id == entity_id) {
            let dt = (at - e.last_at).num_milliseconds() as f64 / 1000.0;
            let dist = e.position.dist(&pos);
            if dt > 0.0 && dist / dt < DWELL_SPEED_MPS {
                e.dwell_seconds += dt;
            } else if dist > 0.0 {
                e.dwell_seconds = 0.0;
            } else if dt > 0.0 {
                e.dwell_seconds += dt;
            }
            e.position = pos;
            e.trajectory.push(pos);
            e.last_at = at;
            e.zone_id = zone_id;
            e.zone_class = zone_class;
        }
    }

    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    /// Distinct corroborating sensors: devices that produced sightings, plus
    /// motion sensors that fired in the entity's zone within the window.
    pub fn distinct_sensors(&self, entity_id: &str) -> usize {
        let Some(e) = self.entity(entity_id) else { return 0 };
        let mut sensors: BTreeSet<&str> = e.devices.iter().map(|d| d.as_str()).collect();
        for (device, zone, at) in &self.motion_events {
            if *zone == e.zone_id || e.trajectory.len() > 1 {
                // A motion hit in any zone the entity has crossed counts if
                // recent; M0 keeps it simple: same zone as first contact or
                // within the corroboration window of the last update.
                if (e.last_at - *at).num_seconds().abs() <= MOTION_CORROBORATION_SECONDS
                    || (*zone == e.zone_id)
                {
                    sensors.insert(device.as_str());
                }
            }
        }
        sensors.len()
    }

    /// A durable handle for a stranger — THIS IS THE PRODUCT (spec §6.1).
    /// Deterministic: derived from the property and first sighting identity,
    /// so replays mint the same handle forever.
    fn mint_unknown_handle(&self, s: &SightingIn) -> String {
        let digest = sha256(format!("{}|{}|{}", self.property_id, s.device_id, s.id).as_bytes());
        format!("unknown:{:02X}{:02X}", digest[0], digest[1])
    }

    fn stamp_expectation(&mut self, entity_id: &str, at: Timestamp) {
        let Some(idx) = self.entities.iter().position(|e| e.id == entity_id) else { return };
        let (identity_id, known, zone_id) = {
            let e = &self.entities[idx];
            (e.identity_id.clone(), e.identity_known, e.zone_id.clone())
        };
        if !known {
            return; // an unknown can never match an identity expectation
        }
        let matched = self.expectations.iter().find(|x| {
            let in_window = at >= x.start && at <= x.end;
            let zone_ok = x.zone_ids.is_empty() || x.zone_ids.contains(&zone_id);
            let who_ok = x.person_id.as_deref() == Some(identity_id.as_str())
                || x.vehicle_id.as_deref() == Some(identity_id.as_str());
            in_window && zone_ok && who_ok
        });
        let e = &mut self.entities[idx];
        match matched {
            Some(x) => {
                e.expected = true;
                e.expectation_id = Some(x.id.clone());
            }
            None => {
                e.expected = false;
                e.expectation_id = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_common::clock::parse_ts;

    fn zones() -> Vec<ZoneDef> {
        vec![
            ZoneDef {
                id: "grounds_north".into(),
                class: ZoneClass::Grounds,
                area: Polygon::from_pairs(&[[200.0, 80.0], [520.0, 80.0], [520.0, 230.0], [200.0, 230.0]]),
            },
            ZoneDef {
                id: "gate_drive".into(),
                class: ZoneClass::Threshold,
                area: Polygon::from_pairs(&[[600.0, 300.0], [680.0, 300.0], [680.0, 380.0], [600.0, 380.0]]),
            },
        ]
    }

    fn s(id: &str, dev: &str, t: &str, conf: f64, x: f64, y: f64) -> SightingIn {
        SightingIn {
            id: id.into(),
            event_id: format!("evt-{id}"),
            device_id: dev.into(),
            at: parse_ts(t).unwrap(),
            class: ObjectClass::Person,
            confidence: conf,
            world_position: Point::new(x, y),
            embedding_of: None,
        }
    }

    #[test]
    fn three_sensors_fuse_to_one_entity_and_beat_any_one() {
        let mut r = Resolver::new("ridgeline", zones(), vec![]);
        r.ingest_motion("S-12", "grounds_north", parse_ts("2026-03-14T03:11:43Z").unwrap());
        let e1 = r.ingest_sighting(s("1", "CAM-04", "2026-03-14T03:11:43.6Z", 0.79, 330.0, 125.0));
        let e2 = r.ingest_sighting(s("2", "CAM-04", "2026-03-14T03:11:44.4Z", 0.84, 352.0, 152.0));
        let e3 = r.ingest_sighting(s("3", "CAM-11", "2026-03-14T03:11:45.4Z", 0.81, 398.0, 205.0));
        assert_eq!(e1, e2);
        assert_eq!(e2, e3);
        assert_eq!(r.entities.len(), 1);
        let e = r.entity(&e1).unwrap();
        assert!(e.confidence >= 0.99, "fused {} — three sensors beat any one", e.confidence);
        assert!(e.identity_id.starts_with("unknown:"), "durable handle: {}", e.identity_id);
        assert!(!e.identity_known);
        assert_eq!(r.distinct_sensors(&e1), 3, "CAM-04 + CAM-11 + S-12 motion");
    }

    #[test]
    fn unknown_handles_are_deterministic_across_replays() {
        let mut a = Resolver::new("ridgeline", zones(), vec![]);
        let mut b = Resolver::new("ridgeline", zones(), vec![]);
        let ea = a.ingest_sighting(s("1", "CAM-04", "2026-03-14T03:11:43.6Z", 0.79, 330.0, 125.0));
        let eb = b.ingest_sighting(s("1", "CAM-04", "2026-03-14T03:11:43.6Z", 0.79, 330.0, 125.0));
        assert_eq!(a.entity(&ea).unwrap().identity_id, b.entity(&eb).unwrap().identity_id);
    }

    #[test]
    fn gallery_match_binds_identity_and_expectation() {
        let expectations = vec![ExpectationDef {
            id: "exp-ana-tue".into(),
            person_id: Some("person:ana".into()),
            vehicle_id: None,
            zone_ids: vec!["gate_drive".into()],
            start: parse_ts("2026-03-17T09:00:00Z").unwrap(),
            end: parse_ts("2026-03-17T13:00:00Z").unwrap(),
        }];
        let mut r = Resolver::new("ridgeline", zones(), expectations);
        let mut sighting = s("1", "CAM-GATE", "2026-03-17T09:05:02Z", 0.91, 640.0, 330.0);
        sighting.embedding_of = Some("person:ana".into());
        let eid = r.ingest_sighting(sighting);
        let e = r.entity(&eid).unwrap();
        assert_eq!(e.identity_id, "person:ana");
        assert!(e.identity_known);
        assert!(e.expected, "the estate manager pre-registered her");
        assert_eq!(e.expectation_id.as_deref(), Some("exp-ana-tue"));
    }

    #[test]
    fn outside_the_window_is_not_expected() {
        let expectations = vec![ExpectationDef {
            id: "exp-ana-tue".into(),
            person_id: Some("person:ana".into()),
            vehicle_id: None,
            zone_ids: vec![],
            start: parse_ts("2026-03-17T09:00:00Z").unwrap(),
            end: parse_ts("2026-03-17T13:00:00Z").unwrap(),
        }];
        let mut r = Resolver::new("ridgeline", zones(), expectations);
        let mut sighting = s("1", "CAM-GATE", "2026-03-17T21:30:00Z", 0.91, 640.0, 330.0);
        sighting.embedding_of = Some("person:ana".into());
        let eid = r.ingest_sighting(sighting);
        assert!(!r.entity(&eid).unwrap().expected, "same person, wrong hour: not expected");
    }

    #[test]
    fn dwell_accumulates_when_stationary() {
        let mut r = Resolver::new("ridgeline", zones(), vec![]);
        let eid = r.ingest_sighting(s("1", "CAM-11", "2026-03-14T03:11:52Z", 0.9, 440.0, 240.0));
        r.advance_entity(&eid, Point::new(440.0, 240.0), parse_ts("2026-03-14T03:12:02Z").unwrap());
        r.advance_entity(&eid, Point::new(440.5, 240.0), parse_ts("2026-03-14T03:12:06Z").unwrap());
        let e = r.entity(&eid).unwrap();
        assert!(e.dwell_seconds >= 13.9, "dwell {}", e.dwell_seconds);
    }
}
