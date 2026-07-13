//! The Governor's audit chain. An AuditRecord is written on EVERY decision
//! path, including DENY (spec §12.1). Hash-chained; a break locks the
//! appliance out of physical actions (I7) — and detecting that break is this
//! module's `verify` function.

use aegis_common::canonical::to_canonical_json;
use aegis_common::clock::Timestamp;
use aegis_common::hash::sha256;
use aegis_common::types::hex_bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: String,
    pub subject_id: String,
    /// A human or a service. Never an LLM in an authorization path (A2).
    pub actor_id: String,
    pub action: String,
    pub rule_version: String,
    pub confidence_at_decision: f32,
    pub at: Timestamp,
    #[serde(with = "hex_bytes")]
    pub prev_hash: Vec<u8>,
    #[serde(with = "hex_bytes")]
    pub hash: Vec<u8>,
}

impl AuditRecord {
    fn content_hash(&self) -> Vec<u8> {
        let mut unsealed = self.clone();
        unsealed.hash = Vec::new();
        sha256(&to_canonical_json(&unsealed)).to_vec()
    }
}

#[derive(Debug, Default)]
pub struct AuditChain {
    records: Vec<AuditRecord>,
}

impl AuditChain {
    pub fn append(&mut self, mut record: AuditRecord) -> String {
        record.prev_hash = self.records.last().map(|r| r.hash.clone()).unwrap_or_default();
        record.hash = record.content_hash();
        let id = record.id.clone();
        self.records.push(record);
        id
    }

    pub fn records(&self) -> &[AuditRecord] {
        &self.records
    }

    pub fn verify(&self) -> bool {
        let mut prev: Vec<u8> = Vec::new();
        for r in &self.records {
            if r.prev_hash != prev || r.hash != r.content_hash() {
                return false;
            }
            prev = r.hash.clone();
        }
        true
    }
}

/// Builder used by the Governor for every path.
pub fn record(
    seq: u64,
    subject_id: &str,
    actor_id: &str,
    action: &str,
    rule_version: &str,
    confidence: f32,
    at: Timestamp,
) -> AuditRecord {
    AuditRecord {
        id: format!("aud-{seq:06}"),
        subject_id: subject_id.to_string(),
        actor_id: actor_id.to_string(),
        action: action.to_string(),
        rule_version: rule_version.to_string(),
        confidence_at_decision: confidence,
        at,
        prev_hash: Vec::new(),
        hash: Vec::new(),
    }
}
