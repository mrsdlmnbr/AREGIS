//! ActionGrant — a short-lived signed capability. NOTHING MOVES WITHOUT ONE.
//! (spec §12.3). Verification lives here too: the asset-adapter and the
//! (simulated) asset firmware call `verify_grant` with the Governor public
//! key THEY hold, so a compromised Governor still cannot mint a grant the
//! asset accepts for rung ≥ ANNOUNCE without an operator's key.

use crate::caps::GRANT_TTL_SECONDS;
use aegis_common::canonical::to_canonical_json;
use aegis_common::clock::Timestamp;
use aegis_common::crypto;
use aegis_common::types::{hex_bytes, AutonomyLevel, EscalationRung};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionGrant {
    pub grant_id: String,
    pub mission_id: String,
    pub asset_id: String,
    pub rung: EscalationRung,
    pub autonomy: AutonomyLevel,
    /// The asset verifies this against its LOADED fence; mismatch → refuse
    /// (G-08).
    #[serde(with = "hex_bytes")]
    pub geofence_hash: Vec<u8>,
    pub not_before: Timestamp,
    /// ≤ 30 s after not_before. Replay after expiry → refuse (G-07).
    pub not_after: Timestamp,
    /// Operator id, or rule id for autonomous rungs. NEVER an LLM (A2).
    pub authorized_by: String,
    pub rule_version: String,
    pub policy_version: String,
    #[serde(with = "hex_bytes")]
    pub governor_signature: Vec<u8>,
    /// REQUIRED for rung >= ANNOUNCE — the second key of the two-key model.
    #[serde(with = "hex_bytes")]
    pub operator_signature: Vec<u8>,
}

impl ActionGrant {
    /// The bytes the Governor signs: the canonical encoding with both
    /// signature fields empty.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut unsigned = self.clone();
        unsigned.governor_signature = Vec::new();
        unsigned.operator_signature = Vec::new();
        to_canonical_json(&unsigned)
    }

    pub fn signature_count(&self) -> usize {
        usize::from(!self.governor_signature.is_empty())
            + usize::from(!self.operator_signature.is_empty())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum GrantRejection {
    #[error("grant TTL exceeds {GRANT_TTL_SECONDS}s")]
    TtlTooLong,
    #[error("grant not yet valid")]
    NotYetValid,
    #[error("grant expired (G-07: a replayed grant is a dead grant)")]
    Expired,
    #[error("geofence hash mismatch (G-08: the fence in the grant is not the fence in the asset)")]
    GeofenceMismatch,
    #[error("governor signature invalid")]
    BadGovernorSignature,
    #[error("operator signature missing for rung >= ANNOUNCE (I1)")]
    MissingOperatorSignature,
}

/// Full onboard verification, exactly as the asset firmware performs it
/// (spec §13): governor signature against the pubkey the asset holds, TTL,
/// fence hash, and the two-key requirement for rung ≥ ANNOUNCE.
///
/// `now` is the verifier's clock — time is an input (A7).
pub fn verify_grant(
    grant: &ActionGrant,
    governor_pubkey: &[u8],
    loaded_geofence_hash: &[u8],
    now: Timestamp,
) -> Result<(), GrantRejection> {
    let ttl = grant.not_after - grant.not_before;
    if ttl > chrono::Duration::seconds(GRANT_TTL_SECONDS) {
        return Err(GrantRejection::TtlTooLong);
    }
    if now < grant.not_before {
        return Err(GrantRejection::NotYetValid);
    }
    if now > grant.not_after {
        return Err(GrantRejection::Expired);
    }
    if grant.geofence_hash != loaded_geofence_hash {
        return Err(GrantRejection::GeofenceMismatch);
    }
    if grant.rung.requires_human_signature() && grant.operator_signature.is_empty() {
        return Err(GrantRejection::MissingOperatorSignature);
    }
    crypto::verify(
        governor_pubkey,
        &grant.signing_bytes(),
        &grant.governor_signature,
    )
    .map_err(|_| GrantRejection::BadGovernorSignature)
}
