//! Driver host — the gateway side of the DAL contract (spec §10.2).
//!
//! Drivers run OUT OF PROCESS and stream provenance-stamped events; the
//! gateway normalises them onto hash-chained topics. M1-dev transport is
//! JSON lines on the driver's stdout (exactly what drivers/sim-camera
//! emits); the local-gRPC transport replaces the framing at M2 without
//! touching the semantics below: declared authority or no ingestion,
//! recorded_at from the appliance clock, never the driver's.
//!
//! A driver that emits an event with no authority_ref is not "a warning" —
//! the stream is terminated and the driver is FAULT. Fail closed: a
//! misbehaving driver loses its link, it does not gain an exception.

use crate::{Gateway, IngestError, RawEvent, RegisteredSource, SourceHealth};
use aegis_common::clock::Clock;
use aegis_common::types::Authority;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::BufRead;
use thiserror::Error;

/// One stamped line from a driver (the drivers/sim-camera output contract).
#[derive(Debug, Clone, Deserialize)]
pub struct DriverLine {
    pub source_id: String,
    pub authority: String,
    pub authority_ref: String,
    #[serde(default)]
    pub attested: bool,
    pub captured_at: String,
    pub payload_type: String,
    #[serde(default)]
    pub payload_b64: String,
    #[serde(default)]
    pub frame_sha256: String,
}

#[derive(Debug, Error)]
pub enum DriverHostError {
    #[error("driver line {line}: not valid JSON: {err}")]
    BadLine { line: usize, err: String },
    #[error("driver line {line}: unknown authority {authority:?} — the set is closed (spec §3.4)")]
    UnknownAuthority { line: usize, authority: String },
    #[error("driver line {line}: bad captured_at: {err}")]
    BadTimestamp { line: usize, err: String },
    #[error("driver line {line}: {err} — driver marked FAULT, stream terminated")]
    Refused { line: usize, err: IngestError },
    #[error("driver stream read failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IngestStats {
    pub events: usize,
    pub sources_registered: usize,
    pub degraded_events: usize,
}

fn parse_authority(s: &str) -> Option<Authority> {
    match s {
        "OWNED" => Some(Authority::Owned),
        "MESH_CONSENTED" => Some(Authority::MeshConsented),
        "PUBLIC_OPEN" => Some(Authority::PublicOpen),
        "LICENSED" => Some(Authority::Licensed),
        "OWNER_DIRECTED" => Some(Authority::OwnerDirected),
        _ => None,
    }
}

/// Consume a driver's stamped event stream into a hash-chained topic.
/// Sources are registered on first sight with the authority THE DRIVER
/// DECLARES — and the gateway's registry refuses an empty ref, which is the
/// structural version of "accessible is not authorized".
///
/// `clock` supplies recorded_at (A7): the appliance clock, never the
/// driver's. The gap between the two is exactly the clock-discipline signal
/// (spec §9.1).
pub fn ingest_driver_stream(
    gw: &mut Gateway,
    topic: &str,
    reader: impl BufRead,
    clock: &dyn Clock,
) -> Result<IngestStats, DriverHostError> {
    let mut stats = IngestStats::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let raw = line?;
        if raw.trim().is_empty() {
            continue;
        }
        let dl: DriverLine = serde_json::from_str(&raw).map_err(|e| DriverHostError::BadLine {
            line: line_no,
            err: e.to_string(),
        })?;

        if !seen.contains(&dl.source_id) {
            let authority = parse_authority(&dl.authority).ok_or_else(|| {
                DriverHostError::UnknownAuthority {
                    line: line_no,
                    authority: dl.authority.clone(),
                }
            })?;
            gw.register_source(RegisteredSource {
                source_id: dl.source_id.clone(),
                authority,
                authority_ref: dl.authority_ref.clone(),
                attested: dl.attested,
            })
            .map_err(|err| DriverHostError::Refused { line: line_no, err })?;
            seen.insert(dl.source_id.clone());
            stats.sources_registered += 1;
        }

        let captured_at = aegis_common::clock::parse_ts(&dl.captured_at).map_err(|e| {
            DriverHostError::BadTimestamp {
                line: line_no,
                err: e.to_string(),
            }
        })?;
        let (_env, health) = gw
            .ingest(
                topic,
                RawEvent {
                    source_id: dl.source_id.clone(),
                    captured_at,
                    payload_type: dl.payload_type.clone(),
                    payload: serde_json::json!({
                        "payload_b64": dl.payload_b64,
                        // Chain of custody starts at the sensor: the driver's
                        // content hash rides in the payload and the envelope
                        // hash seals it into the chain.
                        "frame_sha256": dl.frame_sha256,
                    }),
                },
                clock.now(),
            )
            .map_err(|err| DriverHostError::Refused { line: line_no, err })?;
        stats.events += 1;
        if health == SourceHealth::Degraded {
            stats.degraded_events += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_common::chain;
    use aegis_common::clock::{parse_ts, SimClock};
    use std::io::Cursor;

    // A captured, byte-stable sim-camera stream (its output is deterministic
    // — drivers/sim-camera/main_test.go pins that).
    const STREAM: &str = concat!(
        r#"{"source_id":"CAM-04","authority":"OWNED","authority_ref":"estate-owner-ridgeline","attested":true,"captured_at":"2026-03-14T03:11:43.600Z","payload_type":"video/frame","payload_b64":"ZnJhbWUx","frame_sha256":"a"}"#,
        "\n",
        r#"{"source_id":"CAM-04","authority":"OWNED","authority_ref":"estate-owner-ridgeline","attested":true,"captured_at":"2026-03-14T03:11:44.400Z","payload_type":"video/frame","payload_b64":"ZnJhbWUy","frame_sha256":"b"}"#,
        "\n",
    );

    #[test]
    fn driver_stream_lands_on_a_hash_chained_topic() {
        let mut gw = Gateway::new("ridgeline");
        let clock = SimClock::new(parse_ts("2026-03-14T03:11:44.410Z").unwrap());
        let stats =
            ingest_driver_stream(&mut gw, "raw.device", Cursor::new(STREAM), &clock).unwrap();
        assert_eq!(
            stats,
            IngestStats {
                events: 2,
                sources_registered: 1,
                degraded_events: 1
            }
        );
        let topic = gw.topic("raw.device");
        assert_eq!(topic.len(), 2);
        assert!(chain::verify_chain(topic).is_ok());
        assert_eq!(topic[0].prov.authority_ref, "estate-owner-ridgeline");
        assert!(topic[0].prov.attested);
        // recorded_at is the APPLIANCE clock, not the driver's claim.
        assert_eq!(
            topic[0].prov.recorded_at,
            parse_ts("2026-03-14T03:11:44.410Z").unwrap()
        );
        // The first frame is 810 ms older than the appliance clock →
        // DEGRADED and down-weighted, never silently trusted (spec §9.1).
        assert!(topic[0].prov.clock_offset_ms > 50);
    }

    #[test]
    fn missing_authority_ref_terminates_the_stream() {
        let sneaky = r#"{"source_id":"SNEAKY","authority":"PUBLIC_OPEN","authority_ref":"","captured_at":"2026-03-14T03:11:43Z","payload_type":"video/frame","payload_b64":"eA=="}"#;
        let mut gw = Gateway::new("ridgeline");
        let clock = SimClock::new(parse_ts("2026-03-14T03:11:43Z").unwrap());
        let err =
            ingest_driver_stream(&mut gw, "raw.device", Cursor::new(sneaky), &clock).unwrap_err();
        assert!(matches!(err, DriverHostError::Refused { .. }), "{err}");
        assert!(
            gw.topic("raw.device").is_empty(),
            "nothing ingested before the refusal"
        );
    }

    #[test]
    fn unknown_authority_is_refused_not_defaulted() {
        let odd = r#"{"source_id":"X","authority":"FOUND_IT_ONLINE","authority_ref":"trust-me","captured_at":"2026-03-14T03:11:43Z","payload_type":"video/frame"}"#;
        let mut gw = Gateway::new("ridgeline");
        let clock = SimClock::new(parse_ts("2026-03-14T03:11:43Z").unwrap());
        let err =
            ingest_driver_stream(&mut gw, "raw.device", Cursor::new(odd), &clock).unwrap_err();
        assert!(matches!(err, DriverHostError::UnknownAuthority { .. }));
    }

    #[test]
    fn garbage_line_is_a_driver_fault() {
        let mut gw = Gateway::new("ridgeline");
        let clock = SimClock::new(parse_ts("2026-03-14T03:11:43Z").unwrap());
        let err = ingest_driver_stream(&mut gw, "raw.device", Cursor::new("not json\n"), &clock)
            .unwrap_err();
        assert!(matches!(err, DriverHostError::BadLine { line: 1, .. }));
    }
}
