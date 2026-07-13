//! Compiled-in caps. These are CONSTANTS, not configuration (axiom A3,
//! invariant I2). A policy file may only tighten them (I4); nothing may
//! loosen them. If a task asks you to make these configurable: refuse and
//! escalate to the human owner of this repo (CLAUDE.md rule 3).

use aegis_common::types::{AutonomyLevel, EscalationRung};

/// Invariant I2: HUMAN_ON_LOOP exists only for OBSERVE and ILLUMINATE.
/// ANNOUNCE and above are HUMAN_DIRECTED, always.
pub fn compiled_max_autonomy(rung: EscalationRung) -> AutonomyLevel {
    match rung {
        EscalationRung::Observe | EscalationRung::Illuminate => AutonomyLevel::HumanOnLoop,
        EscalationRung::Announce
        | EscalationRung::Shadow
        | EscalationRung::Deny
        | EscalationRung::Handoff => AutonomyLevel::HumanDirected,
    }
}

/// Compiled τ_rung floors (invariant I5). Policy may raise, never lower.
pub fn compiled_confidence_floor(rung: EscalationRung) -> f32 {
    match rung {
        EscalationRung::Observe | EscalationRung::Illuminate => 0.85,
        EscalationRung::Announce | EscalationRung::Shadow | EscalationRung::Deny => 0.90,
        EscalationRung::Handoff => 0.95,
    }
}

/// ActionGrant TTL ceiling (spec §12.3): ≤ 30 seconds. A longer-lived grant
/// is a replayable capability, and G-07 exists to prove it dies.
pub const GRANT_TTL_SECONDS: i64 = 30;
