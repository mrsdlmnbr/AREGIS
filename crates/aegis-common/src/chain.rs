//! Hash-chained event log (axiom A1, spec §7.1). A gap or a broken link in
//! `decision.audit` is a P0 and locks the appliance out of physical actions
//! until a human clears it (invariant I7).

use crate::canonical::to_canonical_json;
use crate::hash::sha256;
use crate::types::Envelope;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ChainError {
    #[error("chain break at index {index}: prev_hash does not match prior hash")]
    Break { index: usize },
    #[error("hash mismatch at index {index}: content does not match its hash")]
    Tamper { index: usize },
}

/// Compute the content hash of an envelope: SHA-256 over the canonical
/// encoding with the `hash` field zeroed.
pub fn envelope_hash(env: &Envelope) -> Vec<u8> {
    let mut unsealed = env.clone();
    unsealed.hash = Vec::new();
    sha256(&to_canonical_json(&unsealed)).to_vec()
}

/// Seal an envelope onto a chain: sets prev_hash from the chain tip and
/// computes the content hash. Returns the sealed envelope.
pub fn seal(mut env: Envelope, prev: &[u8]) -> Envelope {
    env.prev_hash = prev.to_vec();
    env.hash = envelope_hash(&env);
    env
}

/// Verify a full chain: each link's hash matches its content, and each
/// prev_hash matches the prior link's hash. The first link's prev_hash is
/// the genesis value (empty).
pub fn verify_chain(chain: &[Envelope]) -> Result<(), ChainError> {
    let mut prev: Vec<u8> = Vec::new();
    for (index, env) in chain.iter().enumerate() {
        if env.prev_hash != prev {
            return Err(ChainError::Break { index });
        }
        if env.hash != envelope_hash(env) {
            return Err(ChainError::Tamper { index });
        }
        prev = env.hash.clone();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::parse_ts;
    use crate::types::{Authority, Provenance};

    fn env(id: &str) -> Envelope {
        let at = parse_ts("2026-03-14T03:11:40Z").unwrap();
        Envelope {
            event_id: id.into(),
            property_id: "ridgeline".into(),
            topic: "decision.audit".into(),
            prov: Provenance {
                source_id: "governor".into(),
                authority: Authority::Owned,
                authority_ref: "estate-owner-ridgeline".into(),
                captured_at: at,
                recorded_at: at,
                clock_offset_ms: 0,
                clock_confidence: 1.0,
                attested: true,
                schema_version: "v1".into(),
            },
            payload: serde_json::json!({"n": id}),
            payload_type: "aegis.v1.AuditRecord".into(),
            prev_hash: vec![],
            hash: vec![],
        }
    }

    fn build_chain(n: usize) -> Vec<Envelope> {
        let mut chain: Vec<Envelope> = Vec::new();
        for i in 0..n {
            let prev = chain.last().map(|e| e.hash.clone()).unwrap_or_default();
            chain.push(seal(env(&format!("evt-{i}")), &prev));
        }
        chain
    }

    #[test]
    fn intact_chain_verifies() {
        assert_eq!(verify_chain(&build_chain(5)), Ok(()));
    }

    #[test]
    fn tampered_payload_is_detected() {
        let mut chain = build_chain(5);
        chain[2].payload = serde_json::json!({"n": "evil"});
        assert_eq!(verify_chain(&chain), Err(ChainError::Tamper { index: 2 }));
    }

    #[test]
    fn removed_link_is_detected() {
        let mut chain = build_chain(5);
        chain.remove(2);
        assert_eq!(verify_chain(&chain), Err(ChainError::Break { index: 2 }));
    }

    #[test]
    fn reordered_links_are_detected() {
        let mut chain = build_chain(5);
        chain.swap(1, 3);
        assert!(verify_chain(&chain).is_err());
    }
}
