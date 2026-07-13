//! Policy loading — invariant I4: a policy may only TIGHTEN the compiled-in
//! caps. A policy that attempts to loosen one is REJECTED AT LOAD and the
//! Governor keeps its last-good policy (test vector G-05).

use crate::caps;
use aegis_common::types::{AutonomyLevel, EscalationRung};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub version: String,
    pub jurisdiction: String,
    /// Max autonomy per rung. Absent rung = compiled cap applies.
    #[serde(default)]
    pub max_autonomy_by_rung: BTreeMap<EscalationRung, AutonomyLevel>,
    /// Min entity confidence per rung (τ_rung, invariant I5). Absent rung =
    /// compiled floor applies. May only be raised.
    #[serde(default)]
    pub min_confidence_by_rung: BTreeMap<EscalationRung, f32>,
}

impl Policy {
    /// The Governor's birth state: no overrides, so the compiled caps and
    /// compiled τ floors govern directly (they are already the ceiling a
    /// policy could never exceed — I4).
    pub fn compiled_default() -> Self {
        Policy {
            version: "compiled-default".into(),
            jurisdiction: "unknown-most-restrictive".into(),
            max_autonomy_by_rung: BTreeMap::new(),
            min_confidence_by_rung: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum PolicyRejection {
    #[error("policy {version} LOOSENS autonomy cap for {rung:?}: {requested:?} > compiled {compiled:?} (I4 — rejected at load, running last-good policy)")]
    LoosensAutonomy {
        version: String,
        rung: EscalationRung,
        requested: AutonomyLevel,
        compiled: AutonomyLevel,
    },
    #[error("policy {version} LOWERS confidence threshold for {rung:?}: {requested} < compiled floor {floor} (I4/I5 — rejected at load)")]
    LowersConfidence {
        version: String,
        rung: EscalationRung,
        requested: f32,
        floor: f32,
    },
    #[error("policy version must be non-empty")]
    NoVersion,
}

/// Validate against the compiled caps. Rejection means the Governor keeps its
/// last-good policy — the new file never takes effect.
pub fn validate(policy: &Policy) -> Result<(), PolicyRejection> {
    if policy.version.is_empty() {
        return Err(PolicyRejection::NoVersion);
    }
    for (rung, requested) in &policy.max_autonomy_by_rung {
        let compiled = caps::compiled_max_autonomy(*rung);
        if *requested > compiled {
            return Err(PolicyRejection::LoosensAutonomy {
                version: policy.version.clone(),
                rung: *rung,
                requested: *requested,
                compiled,
            });
        }
    }
    for (rung, requested) in &policy.min_confidence_by_rung {
        let floor = caps::compiled_confidence_floor(*rung);
        if *requested < floor {
            return Err(PolicyRejection::LowersConfidence {
                version: policy.version.clone(),
                rung: *rung,
                requested: *requested,
                floor,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tightening_is_accepted() {
        let mut p = Policy::compiled_default();
        p.version = "v2".into();
        p.max_autonomy_by_rung
            .insert(EscalationRung::Observe, AutonomyLevel::HumanDirected);
        p.min_confidence_by_rung
            .insert(EscalationRung::Handoff, 0.99);
        assert_eq!(validate(&p), Ok(()));
    }

    #[test]
    fn loosening_confidence_is_rejected() {
        let mut p = Policy::compiled_default();
        p.version = "v3".into();
        p.min_confidence_by_rung
            .insert(EscalationRung::Announce, 0.5);
        assert!(matches!(
            validate(&p),
            Err(PolicyRejection::LowersConfidence { .. })
        ));
    }
}
