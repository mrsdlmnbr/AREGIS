//! The simulated world: the real gateway, resolver, Governor, mission engine
//! and asset model, driven by a scenario script. The Governor does not know
//! it is a simulation (spec §17) — it runs the real policy, verifies real
//! signatures, and issues real grants.

use crate::model::*;
use aegis_common::clock::{parse_ts, Clock, SimClock, Timestamp};
use aegis_common::crypto::KeyPair;
use aegis_common::geometry::{Point, Polygon};
use aegis_common::types::{AssetKind, AssetState, Authority, EscalationRung, ObjectClass, Posture};
use anyhow::{anyhow, bail, Context, Result};
use asset_adapter::SimAsset;
use chrono::Timelike;
use gateway::{Gateway, RawEvent, RegisteredSource};
use governor::{approval_message, Governor, Outcome};
use missions::playbook::{EntityFacts, Playbook};
use missions::{DirectAuthorizer, MissionEngine};
use resolver::{ExpectationDef, Resolver, SightingIn, ZoneDef};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const STEP_SECONDS: f64 = 0.1;
const FLEE_SPEED_MPS: f64 = 6.0;
/// Envelope buffer around dock↔target for launch authorization: covers the
/// engagement area the asset may need while observing a moving subject.
const LAUNCH_ENVELOPE_BUFFER_M: f64 = 60.0;
const RETASK_ENVELOPE_BUFFER_M: f64 = 10.0;

#[derive(Debug, Clone)]
pub struct LastDecision {
    pub outcome: String,
    pub rung: EscalationRung,
    pub autonomy: String,
    pub invariant: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlertView {
    pub id: String,
    pub entity_id: String,
    pub severity: i32,
    pub score: f64,
    pub receipt_names: Vec<String>,
    pub pol_note: String,
}

pub struct World {
    pub fixture: Fixture,
    pub scenario: Scenario,
    bin_dir: PathBuf,
    out_dir: PathBuf,

    pub clock: SimClock,
    start: Timestamp,
    posture: Posture,

    pub gateway: Gateway,
    pub resolver: Resolver,
    pub governor: Governor,
    pub engine: MissionEngine,
    pub assets: Vec<SimAsset>,

    operator_keys: BTreeMap<String, KeyPair>,
    forged_key: KeyPair,
    geofence: Polygon,

    pub alerts: BTreeMap<String, AlertView>, // entity id → alert
    alert_seq: u64,
    sighting_seq: u64,
    pub playbook_fired_for: Vec<String>,
    pub last_decision: Option<LastDecision>,
    pub last_request_grant: Option<governor::grant::ActionGrant>,
    pub mission_id: Option<String>,
    tracked_entity: Option<String>,
    dwell_mode: bool,
    last_dwell_tick: Option<Timestamp>,
    flee: Option<FleeState>,
    pub mesh_handoff: Vec<String>,
    pub first_signal_at: Option<Timestamp>,
    pub first_on_station_at: Option<Timestamp>,
    pub evidence_bundle: Option<PathBuf>,
    frames: Vec<(String, Vec<u8>)>,
}

struct FleeState {
    path: Vec<Point>,
    next_waypoint: usize,
}

impl World {
    pub fn new(fixture: Fixture, scenario: Scenario, bin_dir: PathBuf, out_dir: PathBuf) -> Result<Self> {
        let start = parse_ts(&scenario.start_time).context("scenario start_time")?;
        let posture: Posture = scenario.posture.parse().map_err(|e| anyhow!("{e}"))?;
        let geofence = Polygon::from_pairs(&fixture.property.geofence);

        // Gateway with every fixture device registered under the owner's
        // authority. An unregistered device cannot ingest (spec §3.4).
        let mut gw = Gateway::new(&fixture.property.id);
        for d in &fixture.devices {
            gw.register_source(RegisteredSource {
                source_id: d.id.clone(),
                authority: Authority::Owned,
                authority_ref: format!("estate-owner-{}", fixture.property.id),
                attested: d.attested,
            })
            .map_err(|e| anyhow!("{e}"))?;
        }

        // Resolver over the fixture zones + scenario expectations.
        let zones = fixture
            .zones
            .iter()
            .map(|z| {
                Ok(ZoneDef {
                    id: z.id.clone(),
                    class: z.class.parse().map_err(|e| anyhow!("{e:?}"))?,
                    area: Polygon::from_pairs(&z.area),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let expectations = scenario
            .expectations
            .iter()
            .map(|x| {
                Ok(ExpectationDef {
                    id: x.id.clone(),
                    person_id: x.person_id.clone(),
                    vehicle_id: x.vehicle_id.clone(),
                    zone_ids: x.zones.clone(),
                    start: parse_ts(&x.window[0])?,
                    end: parse_ts(&x.window[1])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let resolver = Resolver::new(&fixture.property.id, zones, expectations);

        // The real Governor with the fixture's (sim-only) keys and the
        // property's tightened confidence policy.
        let mut governor = Governor::new(KeyPair::from_seed_hex(&fixture.appliance.governor_seed_hex)?);
        let mut operator_keys = BTreeMap::new();
        for op in &fixture.operators {
            let kp = KeyPair::from_seed_hex(&op.ed25519_seed_hex)?;
            governor.register_operator(&op.id, kp.public_key_bytes());
            operator_keys.insert(op.id.clone(), kp);
        }
        let mut policy = governor::policy::Policy::compiled_default();
        policy.version = format!("{}-1", fixture.property.id);
        policy.jurisdiction = fixture.property.jurisdiction.clone();
        for (rung, tau) in &fixture.tau_rung {
            policy
                .min_confidence_by_rung
                .insert(rung.parse().map_err(|e| anyhow!("{e:?}"))?, *tau);
        }
        governor
            .load_policy(policy, start)
            .map_err(|e| anyhow!("fixture policy rejected: {e}"))?;

        // Playbooks compiled at load; anything invalid refuses to start.
        let engine = MissionEngine::new(Vec::new());

        // Assets present in this scenario, parameterised by the fixture,
        // holding the REAL governor public key and the property fence.
        let gov_pub = governor.public_key();
        let mut assets = Vec::new();
        for sa in &scenario.assets {
            let def = fixture
                .assets
                .iter()
                .find(|a| a.id == sa.id)
                .ok_or_else(|| anyhow!("scenario asset {} not in fixture", sa.id))?;
            let kind = match def.kind.as_str() {
                "AIR" => AssetKind::Air,
                "GROUND" => AssetKind::Ground,
                other => bail!("unknown asset kind {other}"),
            };
            let rungs = def
                .permitted_rungs
                .iter()
                .map(|r| r.parse().map_err(|e| anyhow!("{e:?}")))
                .collect::<Result<Vec<EscalationRung>>>()?;
            let mut asset = SimAsset::new(
                &def.id,
                kind,
                Point::new(def.dock[0], def.dock[1]),
                def.speed_mps,
                def.launch_seconds,
                rungs,
                geofence.clone(),
                gov_pub,
            );
            asset.battery = sa.battery;
            assets.push(asset);
        }

        Ok(Self {
            forged_key: KeyPair::from_seed_hex(&fixture.forged_seed_hex)?,
            clock: SimClock::new(start),
            start,
            posture,
            gateway: gw,
            resolver,
            governor,
            engine,
            assets,
            operator_keys,
            geofence,
            alerts: BTreeMap::new(),
            alert_seq: 0,
            sighting_seq: 0,
            playbook_fired_for: Vec::new(),
            last_decision: None,
            last_request_grant: None,
            mission_id: None,
            tracked_entity: None,
            dwell_mode: false,
            last_dwell_tick: None,
            flee: None,
            mesh_handoff: Vec::new(),
            first_signal_at: None,
            first_on_station_at: None,
            evidence_bundle: None,
            frames: Vec::new(),
            fixture,
            scenario,
            bin_dir,
            out_dir,
        })
    }

    pub fn load_playbooks(&mut self, dir: &std::path::Path) -> Result<()> {
        let mut playbooks: Vec<Playbook> = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                let yaml = std::fs::read_to_string(&path)?;
                playbooks.push(missions::playbook::parse(&yaml).with_context(|| format!("{path:?}"))?);
            }
        }
        self.engine = MissionEngine::new(playbooks);
        Ok(())
    }

    pub fn abs_time(&self, t: f64) -> Timestamp {
        self.start + chrono::Duration::microseconds((t * 1e6).round() as i64)
    }

    pub fn rel_time(&self, at: Timestamp) -> f64 {
        (at - self.start).num_microseconds().unwrap_or(0) as f64 / 1e6
    }

    // ── time stepping ────────────────────────────────────────────────────────

    pub fn step_to(&mut self, target: Timestamp) {
        while self.clock.now() < target {
            let now = (self.clock.now() + chrono::Duration::milliseconds(100)).min(target);
            self.clock.set(now);

            // Scripted entity motion.
            if self.flee.is_some() {
                self.advance_flee(now);
            } else if self.dwell_mode {
                let due = match self.last_dwell_tick {
                    Some(t) => (now - t).num_milliseconds() >= 1000,
                    None => true,
                };
                if due {
                    if let Some(eid) = self.tracked_entity.clone() {
                        let pos = self.resolver.entity(&eid).map(|e| e.position);
                        if let Some(pos) = pos {
                            self.resolver.advance_entity(&eid, pos, now);
                        }
                    }
                    self.last_dwell_tick = Some(now);
                }
            }

            // Actions whose abort window elapsed become executable: hand the
            // grant to the asset. This is the ONLY place a command is issued,
            // and it holds a grant by construction.
            let ready = self.engine.tick(now);
            for action in ready {
                if let Some(grant) = action.grant {
                    let station = self
                        .tracked_entity
                        .as_ref()
                        .and_then(|e| self.resolver.entity(e))
                        .map(|e| e.position);
                    if let Some(station) = station {
                        if let Some(asset) = self.assets.iter_mut().find(|a| a.id == action.asset_id) {
                            // Onboard verification happens inside.
                            let _ = asset.task_observe(&grant, station, now);
                        }
                    }
                }
            }

            // Assets fly.
            for asset in &mut self.assets {
                asset.step(STEP_SECONDS, now);
                if asset.state == AssetState::OnStation && self.first_on_station_at.is_none() {
                    self.first_on_station_at = Some(now);
                }
            }

            // On-station assets keep the subject in frame (inside the fence).
            if self.flee.is_none() {
                if let Some(eid) = &self.tracked_entity {
                    if let Some(pos) = self.resolver.entity(eid).map(|e| e.position) {
                        for asset in &mut self.assets {
                            if asset.state == AssetState::OnStation {
                                asset.track_target(pos);
                            }
                        }
                    }
                }
            }
        }
    }

    fn advance_flee(&mut self, now: Timestamp) {
        let Some(eid) = self.tracked_entity.clone() else { return };
        let Some(entity_pos) = self.resolver.entity(&eid).map(|e| e.position) else { return };
        let mut remaining = FLEE_SPEED_MPS * STEP_SECONDS;
        let mut pos = entity_pos;
        let flee = self.flee.as_mut().unwrap();
        while remaining > 0.0 && flee.next_waypoint < flee.path.len() {
            let wp = flee.path[flee.next_waypoint];
            let d = pos.dist(&wp);
            if d <= remaining {
                pos = wp;
                remaining -= d;
                flee.next_waypoint += 1;
            } else {
                pos = Point::new(pos.x + (wp.x - pos.x) / d * remaining, pos.y + (wp.y - pos.y) / d * remaining);
                remaining = 0.0;
            }
        }
        self.resolver.advance_entity(&eid, pos, now);
        for asset in &mut self.assets {
            // A holding asset stays held: pursuit was denied (I3), so it does
            // not re-acquire the target even while the target is still inside.
            if asset.state == AssetState::OnStation && !asset.geofence_hold {
                asset.track_target(pos);
            }
        }
    }

    // ── script events ────────────────────────────────────────────────────────

    pub fn apply(&mut self, ev: &ScriptEvent) -> Result<()> {
        let at = self.abs_time(ev.t);
        self.step_to(at);
        if self.first_signal_at.is_none() {
            self.first_signal_at = Some(at);
        }
        match ev {
            e if e.event.as_deref() == Some("motion") => self.apply_motion(e, at),
            e if e.event.as_deref() == Some("detection") => self.apply_detection(e, at),
            e if e.entity_reaches.is_some() => self.apply_entity_reaches(e, at),
            e if e.action.as_deref() == Some("request_approve") => self.apply_request_approve(e, at),
            e if e.entity_mode.as_deref() == Some("FLEE") => self.apply_flee(e, at),
            other => bail!("unhandled script event at t={}: {other:?}", ev.t),
        }
    }

    fn apply_motion(&mut self, ev: &ScriptEvent, at: Timestamp) -> Result<()> {
        let device = ev.device.clone().ok_or_else(|| anyhow!("motion without device"))?;
        let zone = ev.zone.clone().unwrap_or_default();
        self.gateway
            .ingest(
                "raw.device",
                RawEvent {
                    source_id: device.clone(),
                    captured_at: at,
                    payload_type: "motion".into(),
                    payload: serde_json::json!({"zone": zone}),
                },
                at + chrono::Duration::milliseconds(5),
            )
            .map_err(|e| anyhow!("{e}"))?;
        self.resolver.ingest_motion(&device, &zone, at);
        Ok(())
    }

    fn apply_detection(&mut self, ev: &ScriptEvent, at: Timestamp) -> Result<()> {
        let device = ev.device.clone().ok_or_else(|| anyhow!("detection without device"))?;
        let class: ObjectClass = ev
            .class
            .as_deref()
            .ok_or_else(|| anyhow!("detection without class"))?
            .parse()
            .map_err(|e| anyhow!("{e:?}"))?;
        let wp = ev.world_position.ok_or_else(|| anyhow!("detection without world_position"))?;
        let confidence = ev.confidence.ok_or_else(|| anyhow!("detection without confidence"))?;

        self.sighting_seq += 1;
        let sighting_id = format!("sig-{:04}", self.sighting_seq);
        let (env, _health) = self
            .gateway
            .ingest(
                "perception.sighting",
                RawEvent {
                    source_id: device.clone(),
                    captured_at: at,
                    payload_type: "aegis.v1.Sighting".into(),
                    payload: serde_json::json!({
                        "sighting_id": sighting_id,
                        "class": class.name(),
                        "confidence": confidence,
                        "world_position": [wp[0], wp[1]],
                    }),
                },
                at + chrono::Duration::milliseconds(5),
            )
            .map_err(|e| anyhow!("{e}"))?;

        // A deterministic pseudo-frame per sighting: what evidence seals.
        self.frames.push((
            format!("frame-{sighting_id}.bin"),
            format!("AEGISFRAME|{}|{}", self.scenario.name, sighting_id).into_bytes(),
        ));

        let eid = self.resolver.ingest_sighting(SightingIn {
            id: sighting_id,
            event_id: env.event_id.clone(),
            device_id: device,
            at,
            class,
            confidence,
            world_position: Point::new(wp[0], wp[1]),
            embedding_of: ev.embedding_of.clone(),
        });
        self.tracked_entity = Some(eid.clone());
        self.assess(&eid, at)?;
        Ok(())
    }

    fn apply_entity_reaches(&mut self, ev: &ScriptEvent, at: Timestamp) -> Result<()> {
        let pos = ev.entity_reaches.unwrap();
        let eid = self
            .tracked_entity
            .clone()
            .ok_or_else(|| anyhow!("entity_reaches before any detection"))?;
        self.resolver.advance_entity(&eid, Point::new(pos[0], pos[1]), at);
        if ev.then.as_deref() == Some("dwell") {
            self.dwell_mode = true;
            self.last_dwell_tick = Some(at);
        }
        self.assess(&eid, at)?;
        Ok(())
    }

    fn apply_request_approve(&mut self, ev: &ScriptEvent, at: Timestamp) -> Result<()> {
        let operator = ev.operator.clone().ok_or_else(|| anyhow!("approve without operator"))?;
        let rung: EscalationRung = ev
            .rung
            .as_deref()
            .ok_or_else(|| anyhow!("approve without rung"))?
            .parse()
            .map_err(|e| anyhow!("{e:?}"))?;
        let mission_id = self.mission_id.clone().unwrap_or_else(|| "msn-000001".to_string());

        // The hold-to-authorize signature: real for `valid`, wrong-key for
        // `forged` (G-03 as a full-stack event).
        let msg = approval_message(&mission_id, rung, at);
        let signature = match ev.signature.as_deref() {
            Some("valid") => self
                .operator_keys
                .get(&operator)
                .ok_or_else(|| anyhow!("unknown operator {operator}"))?
                .sign(&msg),
            Some("forged") => self.forged_key.sign(&msg),
            other => bail!("signature must be valid|forged, got {other:?}"),
        };

        let (asset_id, asset_state, asset_pos) = self
            .assets
            .iter()
            .find(|a| matches!(a.state, AssetState::OnStation | AssetState::Docked))
            .map(|a| (a.id.clone(), a.state, a.position))
            .ok_or_else(|| anyhow!("no taskable asset"))?;
        let confidence = self
            .tracked_entity
            .as_ref()
            .and_then(|e| self.resolver.entity(e))
            .map(|e| e.confidence as f32)
            .unwrap_or(0.0);
        let zone_id = self
            .tracked_entity
            .as_ref()
            .and_then(|e| self.resolver.entity(e))
            .map(|e| e.zone_id.clone())
            .unwrap_or_default();

        // Envelope: SHADOW pursues the flee path; anything else acts in
        // place. The full swept volume, honestly declared (spec §12.1).
        let envelope = if rung == EscalationRung::Shadow {
            let mut pts = vec![asset_pos];
            if let Some(f) = &self.flee {
                pts.extend(f.path.iter().copied().skip(f.next_waypoint.saturating_sub(1)));
                pts.extend(f.path.iter().copied());
            }
            bbox_polygon(&pts, RETASK_ENVELOPE_BUFFER_M)
        } else {
            bbox_polygon(&[asset_pos], RETASK_ENVELOPE_BUFFER_M)
        };

        let decision = self.engine.request_rung(
            &mut DirectAuthorizer(&mut self.governor),
            &mission_id,
            rung,
            &asset_id,
            asset_state,
            confidence,
            self.posture,
            envelope,
            self.geofence.clone(),
            &zone_id,
            at,
            Some(operator),
            Some(signature),
        );
        if let Ok(d) = &decision {
            self.last_decision = Some(LastDecision {
                outcome: d.outcome.name().to_string(),
                rung,
                autonomy: "HUMAN_DIRECTED".into(),
                invariant: d.invariant_violated.clone(),
            });
            if let Outcome::Allow(grant) = &d.outcome {
                self.last_request_grant = Some(grant.clone());
                if rung == EscalationRung::Announce {
                    if let Some(asset) = self.assets.iter_mut().find(|a| a.id == asset_id) {
                        asset
                            .task_announce(grant, at)
                            .map_err(|e| anyhow!("announce refused onboard: {e}"))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_flee(&mut self, ev: &ScriptEvent, at: Timestamp) -> Result<()> {
        let path: Vec<Point> = ev
            .path
            .as_ref()
            .ok_or_else(|| anyhow!("FLEE without path"))?
            .iter()
            .map(|p| Point::new(p[0], p[1]))
            .collect();
        self.dwell_mode = false;
        self.flee = Some(FleeState { path: path.clone(), next_waypoint: 0 });

        // The mission executor asks to keep following. The predicted pursuit
        // trajectory leaves the fence, so the Governor denies (I3), the asset
        // holds at the line with optics masked, and the handoff goes to the
        // mesh. We do not follow anyone off the property. Ever.
        if let Some(mission_id) = self.mission_id.clone() {
            let (asset_id, asset_state, asset_pos) = match self
                .assets
                .iter()
                .find(|a| matches!(a.state, AssetState::OnStation | AssetState::Enroute))
            {
                Some(a) => (a.id.clone(), a.state, a.position),
                None => return Ok(()),
            };
            let mut pts = vec![asset_pos];
            pts.extend(path.iter().copied());
            let envelope = bbox_polygon(&pts, RETASK_ENVELOPE_BUFFER_M);
            let confidence = self
                .tracked_entity
                .as_ref()
                .and_then(|e| self.resolver.entity(e))
                .map(|e| e.confidence as f32)
                .unwrap_or(0.0);
            let req = governor::AuthorizeRequest {
                mission_id,
                rung: EscalationRung::Observe,
                autonomy_requested: aegis_common::types::AutonomyLevel::HumanSupervised,
                asset_id: asset_id.clone(),
                asset_state: if asset_state == AssetState::Enroute { AssetState::OnStation } else { asset_state },
                zone_id: "pursuit".into(),
                trajectory_envelope: envelope,
                geofence: self.geofence.clone(),
                entity_confidence: confidence,
                posture: self.posture,
                at,
                operator_credential_id: None,
                operator_signature: None,
                policy_version: String::new(),
                rule_id: "mission-executor:follow".into(),
            };
            let d = self.governor.authorize(&req);
            self.last_decision = Some(LastDecision {
                outcome: d.outcome.name().to_string(),
                rung: EscalationRung::Observe,
                autonomy: "HUMAN_SUPERVISED".into(),
                invariant: d.invariant_violated.clone(),
            });
            if matches!(d.outcome, Outcome::Deny { .. }) {
                // Pursuit denied: hold at the line, mask, hand off to mesh.
                if let Some(asset) = self.assets.iter_mut().find(|a| a.id == asset_id) {
                    asset.geofence_hold = true;
                    asset.optics_masked = true;
                }
                self.compute_mesh_handoff(&path);
            }
        }
        Ok(())
    }

    fn compute_mesh_handoff(&mut self, path: &[Point]) {
        // Where does the flee path cross the fence?
        let mut crossing: Option<Point> = None;
        let mut prev: Option<Point> = None;
        for p in path {
            if let Some(a) = prev {
                if let Some(t) = self.geofence.first_boundary_crossing(&a, p) {
                    crossing = Some(Point::new(a.x + t * (p.x - a.x), a.y + t * (p.y - a.y)));
                    break;
                }
            }
            prev = Some(*p);
        }
        let Some(cross) = crossing else { return };
        let mut nodes: Vec<String> = self
            .fixture
            .mesh_nodes
            .iter()
            .filter(|n| Point::new(n.position[0], n.position[1]).dist(&cross) <= self.fixture.mesh_handoff_range_m)
            .map(|n| n.id.clone())
            .collect();
        nodes.sort();
        self.mesh_handoff = nodes;
    }

    // ── assessment: score → alert → playbook → mission ──────────────────────

    fn assess(&mut self, entity_id: &str, at: Timestamp) -> Result<()> {
        let (facts, sensors, event_ids) = {
            let e = self
                .resolver
                .entity(entity_id)
                .ok_or_else(|| anyhow!("assess: unknown entity"))?;
            (
                EntityFacts {
                    class: e.class,
                    identity_known: e.identity_known,
                    zone_class: e.zone_class,
                    confidence: e.confidence,
                    expected: e.expected,
                    dwell_seconds: e.dwell_seconds,
                },
                self.resolver.distinct_sensors(entity_id) as u32,
                e.contributing_event_ids.clone(),
            )
        };

        let local_hour = at.hour(); // fixture properties run UTC in M0
        let night = local_hour >= 22 || local_hour < 6;
        let pol = &self.fixture.pol_baseline;
        let anomaly = match (facts.class, facts.identity_known) {
            (ObjectClass::Animal, _) => pol.anomaly.animal,
            (ObjectClass::Person, true) => pol.anomaly.known_person,
            (ObjectClass::Person, false) => {
                if night {
                    pol.anomaly.unknown_person_night
                } else {
                    pol.anomaly.unknown_person_day
                }
            }
            (ObjectClass::Vehicle, true) => pol.anomaly.vehicle_known,
            (ObjectClass::Vehicle, false) => pol.anomaly.vehicle_unknown,
            _ => 0.3,
        };
        let input = ThreatInput {
            schema: "aegis.sim.threat/v1".into(),
            object_class: facts.class.name().into(),
            identity_known: facts.identity_known,
            zone_class: facts.zone_class.name().into(),
            posture: self.posture.name().into(),
            local_hour,
            expected: facts.expected,
            entity_confidence: facts.confidence,
            distinct_sensors: sensors,
            mesh_corroborations: 0,
            dwell_seconds: facts.dwell_seconds,
            pol: ThreatPol {
                available: pol.available,
                anomaly,
                note: format!(
                    "last unexpected perimeter entity: {} days ago",
                    pol.last_unexpected_perimeter_days
                ),
            },
            all_contributing_unattested: false,
        };
        let mut output = self.run_threat_cli(&input)?;

        // Playbook escalation rules (e.g. dwell at a threshold → severity 5).
        for pb in self.engine.playbooks().to_vec() {
            for rule in &pb.escalate {
                if missions::playbook::eval_condition(&rule.condition, &facts, self.posture).unwrap_or(false) {
                    if let Some(sev) = rule.then.strip_prefix("severity = ") {
                        if let Ok(sev) = sev.trim().parse::<i32>() {
                            output.severity = output.severity.max(sev.min(5));
                        }
                    }
                }
            }
        }

        let alert_id = match self.alerts.get(entity_id) {
            Some(a) => a.id.clone(),
            None => {
                self.alert_seq += 1;
                format!("alert-{}", self.alert_seq)
            }
        };
        self.alerts.insert(
            entity_id.to_string(),
            AlertView {
                id: alert_id.clone(),
                entity_id: entity_id.to_string(),
                severity: output.severity,
                score: output.score,
                receipt_names: output.receipt.iter().map(|r| r.name.clone()).collect(),
                pol_note: output.pol_note.clone(),
            },
        );

        // Playbook trigger — once per entity.
        if !self.playbook_fired_for.contains(&entity_id.to_string()) {
            let fired = self.engine.matching_playbook(&facts, self.posture).map(|pb| pb.name.clone());
            if let Some(pb_name) = fired {
                self.playbook_fired_for.push(entity_id.to_string());
                self.open_mission_for(&pb_name, &alert_id, entity_id, &facts, at)?;
                let seal = self
                    .engine
                    .playbooks()
                    .iter()
                    .find(|p| p.name == pb_name)
                    .map(|p| p.evidence.seal_on_trigger)
                    .unwrap_or(false);
                if seal {
                    self.seal_evidence(&alert_id, &event_ids, at)?;
                }
            }
        }
        Ok(())
    }

    fn open_mission_for(
        &mut self,
        playbook_name: &str,
        alert_id: &str,
        entity_id: &str,
        facts: &EntityFacts,
        at: Timestamp,
    ) -> Result<()> {
        // Pick the asset the playbook asks for: any available air, else ground.
        let asset = self
            .assets
            .iter()
            .find(|a| a.kind == AssetKind::Air && a.state == AssetState::Docked)
            .or_else(|| self.assets.iter().find(|a| a.state == AssetState::Docked))
            .ok_or_else(|| anyhow!("no available asset"))?;
        let (asset_id, asset_state, dock) = (asset.id.clone(), asset.state, asset.dock);
        let entity_pos = self
            .resolver
            .entity(entity_id)
            .map(|e| e.position)
            .ok_or_else(|| anyhow!("no entity"))?;
        let zone_id = self.resolver.entity(entity_id).map(|e| e.zone_id.clone()).unwrap_or_default();
        let envelope = bbox_polygon(&[dock, entity_pos], LAUNCH_ENVELOPE_BUFFER_M);

        let mid = self.engine.open_mission(
            &mut DirectAuthorizer(&mut self.governor),
            playbook_name,
            alert_id,
            &asset_id,
            asset_state,
            facts,
            self.posture,
            envelope,
            self.geofence.clone(),
            &zone_id,
            at,
        );
        // Record the launch decision for the assertion engine.
        if let Some(m) = self.engine.mission(&mid) {
            if let Some(a) = m.actions.first() {
                self.last_decision = Some(LastDecision {
                    outcome: match a.state {
                        missions::ActionState::CountingDown | missions::ActionState::Executing => "ALLOW".into(),
                        missions::ActionState::AwaitingApproval => "REQUIRE_APPROVAL".into(),
                        missions::ActionState::Denied => "DENY".into(),
                        _ => "NONE".into(),
                    },
                    rung: a.rung,
                    autonomy: a.autonomy.name().into(),
                    invariant: None,
                });
            }
        }
        self.mission_id = Some(mid);
        Ok(())
    }

    // ── governor probe (assertions with operator_signature: absent) ─────────

    pub fn probe_authorize(&mut self, rung: EscalationRung, at: Timestamp) -> governor::Decision {
        let (asset_id, asset_state, asset_pos) = self
            .assets
            .iter()
            .find(|a| matches!(a.state, AssetState::OnStation | AssetState::Docked))
            .map(|a| (a.id.clone(), a.state, a.position))
            .unwrap_or(("BEE-01".into(), AssetState::Docked, Point::new(0.0, 0.0)));
        let confidence = self
            .tracked_entity
            .as_ref()
            .and_then(|e| self.resolver.entity(e))
            .map(|e| e.confidence as f32)
            .unwrap_or(0.0);
        let req = governor::AuthorizeRequest {
            mission_id: self.mission_id.clone().unwrap_or_default(),
            rung,
            autonomy_requested: aegis_common::types::AutonomyLevel::HumanDirected,
            asset_id,
            asset_state,
            zone_id: "probe".into(),
            trajectory_envelope: bbox_polygon(&[asset_pos], RETASK_ENVELOPE_BUFFER_M),
            geofence: self.geofence.clone(),
            entity_confidence: confidence,
            posture: self.posture,
            at,
            operator_credential_id: None,
            operator_signature: None,
            policy_version: String::new(),
            rule_id: "assertion-probe".into(),
        };
        self.governor.authorize(&req)
    }

    // ── evidence ─────────────────────────────────────────────────────────────

    fn seal_evidence(&mut self, alert_id: &str, event_ids: &[String], at: Timestamp) -> Result<()> {
        use base64::Engine as _;
        let bundle_dir = self.out_dir.join(&self.scenario.name).join("evidence");
        std::fs::create_dir_all(&bundle_dir)?;
        let req = SealRequest {
            schema: "aegis.sim.evidence/v1".into(),
            incident_id: format!("{}/{}", self.scenario.name, alert_id),
            property_id: self.fixture.property.id.clone(),
            sealed_at: at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            sealed_by: format!("appliance:{}", self.fixture.property.id),
            event_ids: event_ids.to_vec(),
            audit_record_ids: self.governor.audit_records().iter().map(|r| r.id.clone()).collect(),
            media: self
                .frames
                .iter()
                .map(|(name, bytes)| SealMedia {
                    name: name.clone(),
                    b64: base64::engine::general_purpose::STANDARD.encode(bytes),
                })
                .collect(),
            signing_key_seed_hex: self.fixture.appliance.evidence_seed_hex.clone(),
        };
        let mut child = Command::new(self.bin_dir.join("evidence-cli"))
            .arg("seal")
            .arg("--out")
            .arg(&bundle_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn evidence-cli")?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&serde_json::to_vec(&req)?)?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!("evidence-cli seal failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        self.evidence_bundle = Some(bundle_dir);
        Ok(())
    }

    pub fn verify_evidence(&self) -> Result<VerifyResult> {
        let bundle = self.evidence_bundle.as_ref().ok_or_else(|| anyhow!("no evidence bundle"))?;
        let out = Command::new(self.bin_dir.join("evidence-verify"))
            .arg(bundle)
            .output()
            .context("spawn evidence-verify")?;
        Ok(serde_json::from_slice(&out.stdout)?)
    }

    fn run_threat_cli(&self, input: &ThreatInput) -> Result<ThreatOutput> {
        let mut child = Command::new(self.bin_dir.join("threat-cli"))
            .arg("score")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn threat-cli (run `make go-build` first)")?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&serde_json::to_vec(input)?)?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!("threat-cli failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(serde_json::from_slice(&out.stdout)?)
    }

    // ── views for the assertion engine ───────────────────────────────────────

    pub fn asset(&self, id: &str) -> Option<&SimAsset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn tracked_alert(&self) -> Option<&AlertView> {
        self.tracked_entity.as_ref().and_then(|e| self.alerts.get(e))
    }

    pub fn tracked(&self) -> Option<&resolver::Entity> {
        self.tracked_entity.as_ref().and_then(|e| self.resolver.entity(e))
    }

    pub fn geofence(&self) -> &Polygon {
        &self.geofence
    }

    pub fn announcements_made(&self) -> u32 {
        self.assets.iter().map(|a| a.announcements_made).sum()
    }

    pub fn geofence_breaches(&self) -> u32 {
        self.assets.iter().map(|a| a.geofence_breaches).sum()
    }
}

pub fn bbox_polygon(points: &[Point], buffer: f64) -> Polygon {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    Polygon(vec![
        Point::new(min_x - buffer, min_y - buffer),
        Point::new(max_x + buffer, min_y - buffer),
        Point::new(max_x + buffer, max_y + buffer),
        Point::new(min_x - buffer, max_y + buffer),
    ])
}
