//! gateway — normalise everything to an Envelope, stamp provenance,
//! discipline clocks, reject anything without valid authority (spec §9.1).
//!
//! The one rule that keeps the company alive (spec §3.4): an event without a
//! valid `authority_ref` is REJECTED. Not queued, not flagged — rejected and
//! logged. Accessible is not authorized.

use aegis_common::chain;
use aegis_common::clock::Timestamp;
use aegis_common::types::{Authority, Envelope, Provenance};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

/// Clock offset beyond which a device is DEGRADED and its sightings are
/// down-weighted in the resolver — never silently trusted (spec §9.1).
pub const CLOCK_OFFSET_DEGRADED_MS: i32 = 50;

#[derive(Debug, Error, PartialEq)]
pub enum IngestError {
    #[error("no authority_ref on event from '{source_id}' — rejected (spec §3.4: no authority, no ingestion)")]
    NoAuthorityRef { source_id: String },
    #[error("unknown source '{0}' — a device we did not register cannot claim authority")]
    UnknownSource(String),
}

#[derive(Debug, Clone)]
pub struct RegisteredSource {
    pub source_id: String,
    pub authority: Authority,
    pub authority_ref: String,
    pub attested: bool,
}

/// A raw reading arriving from a driver, before provenance stamping.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub source_id: String,
    pub captured_at: Timestamp,
    pub payload_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceHealth {
    Ok,
    Degraded,
}

/// The gateway: per-topic hash chains, a source registry, clock discipline.
pub struct Gateway {
    property_id: String,
    sources: BTreeMap<String, RegisteredSource>,
    chains: BTreeMap<String, Vec<Envelope>>,
    event_seq: u64,
    pub degraded_sources: Vec<String>,
}

impl Gateway {
    pub fn new(property_id: &str) -> Self {
        Self {
            property_id: property_id.to_string(),
            sources: BTreeMap::new(),
            chains: BTreeMap::new(),
            event_seq: 0,
            degraded_sources: Vec::new(),
        }
    }

    /// Registration is where authority is established. A source registered
    /// with an empty authority_ref is refused at the door.
    pub fn register_source(&mut self, source: RegisteredSource) -> Result<(), IngestError> {
        if source.authority_ref.trim().is_empty() {
            return Err(IngestError::NoAuthorityRef {
                source_id: source.source_id,
            });
        }
        self.sources.insert(source.source_id.clone(), source);
        Ok(())
    }

    /// Normalise a raw event into a provenance-stamped, hash-chained
    /// Envelope on `topic`. `recorded_at` is the appliance clock — passed
    /// in, because time is an input (A7).
    pub fn ingest(
        &mut self,
        topic: &str,
        raw: RawEvent,
        recorded_at: Timestamp,
    ) -> Result<(Envelope, SourceHealth), IngestError> {
        let source = self
            .sources
            .get(&raw.source_id)
            .ok_or_else(|| IngestError::UnknownSource(raw.source_id.clone()))?
            .clone();
        // Defence in depth: the registry refused empty refs already, but a
        // registry bug must not become an ingestion path.
        if source.authority_ref.trim().is_empty() {
            return Err(IngestError::NoAuthorityRef {
                source_id: source.source_id,
            });
        }

        let offset_ms = (recorded_at - raw.captured_at).num_milliseconds() as i32;
        let health = if offset_ms.abs() > CLOCK_OFFSET_DEGRADED_MS {
            if !self.degraded_sources.contains(&raw.source_id) {
                self.degraded_sources.push(raw.source_id.clone());
            }
            SourceHealth::Degraded
        } else {
            SourceHealth::Ok
        };

        self.event_seq += 1;
        let env = Envelope {
            event_id: format!("evt-{:06}", self.event_seq),
            property_id: self.property_id.clone(),
            topic: topic.to_string(),
            prov: Provenance {
                source_id: source.source_id.clone(),
                authority: source.authority,
                authority_ref: source.authority_ref.clone(),
                captured_at: raw.captured_at,
                recorded_at,
                clock_offset_ms: offset_ms,
                clock_confidence: if health == SourceHealth::Ok { 1.0 } else { 0.5 },
                attested: source.attested,
                schema_version: "aegis.v1".to_string(),
            },
            payload: raw.payload,
            payload_type: raw.payload_type,
            prev_hash: Vec::new(),
            hash: Vec::new(),
        };
        let chain = self.chains.entry(topic.to_string()).or_default();
        let prev = chain.last().map(|e| e.hash.clone()).unwrap_or_default();
        let sealed = chain::seal(env, &prev);
        chain.push(sealed.clone());
        Ok((sealed, health))
    }

    pub fn topic(&self, topic: &str) -> &[Envelope] {
        self.chains.get(topic).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn verify_topic_chain(&self, topic: &str) -> bool {
        chain::verify_chain(self.topic(topic)).is_ok()
    }

    pub fn all_topics(&self) -> impl Iterator<Item = (&String, &Vec<Envelope>)> {
        self.chains.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_common::clock::parse_ts;

    fn cam() -> RegisteredSource {
        RegisteredSource {
            source_id: "CAM-04".into(),
            authority: Authority::Owned,
            authority_ref: "estate-owner-ridgeline".into(),
            attested: true,
        }
    }

    fn raw(at: &str) -> RawEvent {
        RawEvent {
            source_id: "CAM-04".into(),
            captured_at: parse_ts(at).unwrap(),
            payload_type: "detection".into(),
            payload: serde_json::json!({"class": "PERSON"}),
        }
    }

    #[test]
    fn no_authority_ref_no_ingestion() {
        let mut gw = Gateway::new("ridgeline");
        let err = gw
            .register_source(RegisteredSource {
                source_id: "SNEAKY-FEED".into(),
                authority: Authority::PublicOpen,
                authority_ref: "  ".into(), // "it's technically accessible"
                attested: false,
            })
            .unwrap_err();
        assert!(matches!(err, IngestError::NoAuthorityRef { .. }));
    }

    #[test]
    fn unknown_source_is_rejected() {
        let mut gw = Gateway::new("ridgeline");
        let err = gw
            .ingest(
                "raw.device",
                raw("2026-03-14T03:11:43Z"),
                parse_ts("2026-03-14T03:11:43.010Z").unwrap(),
            )
            .unwrap_err();
        assert!(matches!(err, IngestError::UnknownSource(_)));
    }

    #[test]
    fn envelopes_are_hash_chained_per_topic() {
        let mut gw = Gateway::new("ridgeline");
        gw.register_source(cam()).unwrap();
        for i in 0..5 {
            let at = parse_ts("2026-03-14T03:11:43Z").unwrap() + chrono::Duration::seconds(i);
            gw.ingest(
                "raw.device",
                RawEvent {
                    captured_at: at,
                    ..raw("2026-03-14T03:11:43Z")
                },
                at + chrono::Duration::milliseconds(8),
            )
            .unwrap();
        }
        assert!(gw.verify_topic_chain("raw.device"));
        assert_eq!(gw.topic("raw.device").len(), 5);
        assert_eq!(
            gw.topic("raw.device")[0].prov.authority_ref,
            "estate-owner-ridgeline"
        );
    }

    #[test]
    fn clock_skew_degrades_never_silently_trusts() {
        let mut gw = Gateway::new("ridgeline");
        gw.register_source(cam()).unwrap();
        let captured = parse_ts("2026-03-14T03:11:43Z").unwrap();
        // 5 s skew (adversarial suite case): flagged DEGRADED, confidence cut.
        let (env, health) = gw
            .ingest(
                "raw.device",
                RawEvent {
                    captured_at: captured,
                    ..raw("2026-03-14T03:11:43Z")
                },
                captured + chrono::Duration::seconds(5),
            )
            .unwrap();
        assert_eq!(health, SourceHealth::Degraded);
        assert_eq!(env.prov.clock_offset_ms, 5000);
        assert!(env.prov.clock_confidence < 1.0);
        assert_eq!(gw.degraded_sources, vec!["CAM-04".to_string()]);
    }
}
