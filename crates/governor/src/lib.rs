//! ★ The Autonomy Governor — the only path by which anything physical moves.
//!
//! Design rules (spec §12): small, deterministic, no network calls to models,
//! fail-closed on every doubt. Invariants I1–I9 are compiled in; no
//! configuration may override them. One engineer owns this crate; every
//! change is reviewed by two humans.
//!
//! Decision order is DENY-first: global fail-closed conditions (I7, I8), then
//! hard denials (I6, I3, forged signatures), then the human-required paths
//! (I2/I4 caps, I1 signatures, I5 confidence), and only then ALLOW. An
//! AuditRecord is written on EVERY path, including DENY.

pub mod audit;
pub mod caps;
pub mod grant;
pub mod policy;

use aegis_common::canonical::to_canonical_json;
use aegis_common::clock::Timestamp;
use aegis_common::crypto::{self, KeyPair};
use aegis_common::geometry::Polygon;
use aegis_common::hash::sha256;
use aegis_common::types::{AssetState, AutonomyLevel, EscalationRung, Posture};
use audit::AuditChain;
use grant::ActionGrant;
use policy::{Policy, PolicyRejection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    pub mission_id: String,
    pub rung: EscalationRung,
    pub autonomy_requested: AutonomyLevel,
    pub asset_id: String,
    pub asset_state: AssetState,
    pub zone_id: String,
    /// The full swept volume, not a waypoint (spec §12.1).
    pub trajectory_envelope: Polygon,
    pub geofence: Polygon,
    pub entity_confidence: f32,
    pub posture: Posture,
    /// Time is an INPUT, never an ambient (A7).
    pub at: Timestamp,
    pub operator_credential_id: Option<String>,
    pub operator_signature: Option<Vec<u8>>,
    pub policy_version: String,
    /// The playbook rule requesting this action — becomes `authorized_by`
    /// for autonomous rungs. Never an LLM (A2).
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq)]
// The grant makes Allow large; boxing it would put an allocation between the
// decision and its capability for no safety gain. Deliberate.
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    Allow(ActionGrant),
    RequireApproval { reason: String },
    Deny { reason: String },
}

impl Outcome {
    pub fn name(&self) -> &'static str {
        match self {
            Outcome::Allow(_) => "ALLOW",
            Outcome::RequireApproval { .. } => "REQUIRE_APPROVAL",
            Outcome::Deny { .. } => "DENY",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub decision_id: String,
    pub outcome: Outcome,
    /// e.g. "I3: trajectory leaves geofence". None on ALLOW.
    pub invariant_violated: Option<String>,
    pub audit_record_id: String,
}

/// P0 security events (e.g. a forged operator signature, G-03). The
/// monitoring layer pages on every one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub kind: String,
    pub detail: String,
    pub at: Timestamp,
}

#[derive(Debug, Clone, Default)]
pub struct Health {
    pub log_chain_broken: bool,
    pub clock_skew_detected: bool,
    pub degraded: bool,
}

pub struct Governor {
    key: KeyPair,
    /// credential id → Ed25519 public key. Hardware tokens in production.
    operators: BTreeMap<String, [u8; 32]>,
    policy: Policy,
    health: Health,
    audit: AuditChain,
    security_events: Vec<SecurityEvent>,
    decision_seq: u64,
    grant_seq: u64,
    audit_seq: u64,
}

/// The canonical bytes an operator signs for hold-to-authorize. The console
/// produces this with the operator's hardware token; the Governor
/// reconstructs it and verifies. Public so missions/console/harness agree.
pub fn approval_message(mission_id: &str, rung: EscalationRung, at: Timestamp) -> Vec<u8> {
    to_canonical_json(&serde_json::json!({
        "action": "approve",
        "mission_id": mission_id,
        "rung": rung.name(),
        "at": at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
    }))
}

pub fn geofence_hash(fence: &Polygon) -> Vec<u8> {
    sha256(&to_canonical_json(fence)).to_vec()
}

impl Governor {
    pub fn new(key: KeyPair) -> Self {
        Self {
            key,
            operators: BTreeMap::new(),
            policy: Policy::compiled_default(),
            health: Health::default(),
            audit: AuditChain::default(),
            security_events: Vec::new(),
            decision_seq: 0,
            grant_seq: 0,
            audit_seq: 0,
        }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key.public_key_bytes()
    }

    pub fn register_operator(&mut self, credential_id: &str, pubkey: [u8; 32]) {
        self.operators.insert(credential_id.to_string(), pubkey);
    }

    /// I4/G-05: a policy that loosens anything is rejected AT LOAD and the
    /// last-good policy stays in force.
    pub fn load_policy(&mut self, candidate: Policy, at: Timestamp) -> Result<(), PolicyRejection> {
        match policy::validate(&candidate) {
            Ok(()) => {
                self.audit_append(
                    "policy",
                    "system",
                    &format!("policy:loaded:{}", candidate.version),
                    &candidate.version,
                    1.0,
                    at,
                );
                self.policy = candidate;
                Ok(())
            }
            Err(rejection) => {
                let last_good = self.policy.version.clone();
                self.audit_append(
                    "policy",
                    "system",
                    &format!("policy:rejected:{rejection}"),
                    &last_good,
                    1.0,
                    at,
                );
                Err(rejection)
            }
        }
    }

    pub fn policy_version(&self) -> &str {
        &self.policy.version
    }

    pub fn set_log_chain_broken(&mut self, broken: bool) {
        self.health.log_chain_broken = broken;
    }

    pub fn set_clock_skew(&mut self, skew: bool) {
        self.health.clock_skew_detected = skew;
    }

    pub fn set_degraded(&mut self, degraded: bool) {
        self.health.degraded = degraded;
    }

    /// Clearing a chain break is a human act and an audit record (I7).
    pub fn clear_log_chain_break(&mut self, operator_id: &str, at: Timestamp) {
        self.audit_append(
            "health",
            operator_id,
            "health:chain_break_cleared",
            &self.policy.version.clone(),
            1.0,
            at,
        );
        self.health.log_chain_broken = false;
    }

    pub fn audit_records(&self) -> &[audit::AuditRecord] {
        self.audit.records()
    }

    pub fn audit_chain_intact(&self) -> bool {
        self.audit.verify()
    }

    pub fn security_events(&self) -> &[SecurityEvent] {
        &self.security_events
    }

    /// The decision function (spec §12.1). Deterministic; total; every path
    /// audited.
    pub fn authorize(&mut self, req: &AuthorizeRequest) -> Decision {
        self.decision_seq += 1;
        let decision_id = format!("dec-{:06}", self.decision_seq);

        // ── verify any presented operator signature FIRST: a forged
        // signature is a security event regardless of what else is wrong
        // (G-03), and a valid one is what later steps consume.
        let mut operator_verified: Option<String> = None;
        if let Some(sig) = &req.operator_signature {
            let msg = approval_message(&req.mission_id, req.rung, req.at);
            let cred = req.operator_credential_id.clone().unwrap_or_default();
            let valid = self
                .operators
                .get(&cred)
                .map(|pk| crypto::verify(pk, &msg, sig).is_ok())
                .unwrap_or(false);
            if valid {
                operator_verified = Some(cred);
            } else {
                self.security_events.push(SecurityEvent {
                    kind: "forged_signature".into(),
                    detail: format!(
                        "credential '{}' presented an invalid signature for {} on {}",
                        cred,
                        req.rung.name(),
                        req.mission_id
                    ),
                    at: req.at,
                });
                return self.finish(
                    decision_id,
                    req,
                    Outcome::Deny {
                        reason:
                            "operator signature failed verification — treated as forgery, P0 security event"
                                .into(),
                    },
                    Some("I1: operator signature invalid".into()),
                );
            }
        }

        // ── I7: broken log chain or clock skew ⟹ DENY all physical actions.
        if self.health.log_chain_broken || self.health.clock_skew_detected {
            return self.finish(
                decision_id,
                req,
                Outcome::Deny {
                    reason:
                        "log chain break / clock skew — all physical actions denied until a human clears it"
                            .into(),
                },
                Some("I7: log-chain break or clock skew".into()),
            );
        }

        // ── I8: degraded, policy unverifiable, any doubt ⟹ DENY.
        if self.health.degraded {
            return self.finish(
                decision_id,
                req,
                Outcome::Deny {
                    reason: "governor degraded — fail closed".into(),
                },
                Some("I8: governor degraded".into()),
            );
        }

        // ── I6: asset must be ready. The spec's READY maps to DOCKED
        // (healthy, ready to launch) in the AssetState enum; ON_STATION
        // continues an engagement. Everything else is DENY.
        if !matches!(req.asset_state, AssetState::Docked | AssetState::OnStation) {
            return self.finish(
                decision_id,
                req,
                Outcome::Deny {
                    reason: format!(
                        "asset {} in state {} — not taskable",
                        req.asset_id,
                        req.asset_state.name()
                    ),
                },
                Some("I6: asset not READY/ON_STATION".into()),
            );
        }

        // ── I3: the trajectory envelope must be wholly inside the geofence.
        // An empty or degenerate envelope is OUTSIDE (fail closed).
        if !req.geofence.contains_polygon(&req.trajectory_envelope) {
            return self.finish(
                decision_id,
                req,
                Outcome::Deny {
                    reason: "trajectory envelope leaves the geofence".into(),
                },
                Some("I3: trajectory leaves geofence".into()),
            );
        }

        // ── I2/I4: effective cap = min(compiled, policy). Requests above the
        // cap are not autonomous; they fall to the human path.
        let compiled_cap = caps::compiled_max_autonomy(req.rung);
        let policy_cap = self
            .policy
            .max_autonomy_by_rung
            .get(&req.rung)
            .copied()
            .unwrap_or(compiled_cap);
        let effective_cap = compiled_cap.min(policy_cap);
        let capped = req.autonomy_requested > effective_cap;
        if capped && operator_verified.is_none() {
            let invariant = if req.autonomy_requested == AutonomyLevel::HumanOnLoop
                && req.rung > EscalationRung::Illuminate
            {
                "I2: HUMAN_ON_LOOP is capped at ILLUMINATE"
            } else {
                "I4: requested autonomy exceeds effective cap"
            };
            return self.finish(
                decision_id,
                req,
                Outcome::RequireApproval {
                    reason: format!(
                        "requested {} exceeds cap {} for {} — a human must direct this",
                        req.autonomy_requested.name(),
                        effective_cap.name(),
                        req.rung.name()
                    ),
                },
                Some(invariant.into()),
            );
        }

        // ── I1: ANNOUNCE and above require a verified human signature.
        if req.rung.requires_human_signature() && operator_verified.is_none() {
            return self.finish(
                decision_id,
                req,
                Outcome::RequireApproval {
                    reason: format!(
                        "{} requires an operator signature (hold-to-authorize)",
                        req.rung.name()
                    ),
                },
                Some("I1: operator signature required".into()),
            );
        }

        // ── I5: never auto-act on a weak signal. A verified operator IS the
        // approval; without one, low confidence goes to a human.
        let tau = self
            .policy
            .min_confidence_by_rung
            .get(&req.rung)
            .copied()
            .unwrap_or_else(|| caps::compiled_confidence_floor(req.rung))
            .max(caps::compiled_confidence_floor(req.rung));
        if req.entity_confidence < tau && operator_verified.is_none() {
            return self.finish(
                decision_id,
                req,
                Outcome::RequireApproval {
                    reason: format!(
                        "entity confidence {:.2} below τ_{} = {:.2}",
                        req.entity_confidence,
                        req.rung.name(),
                        tau
                    ),
                },
                Some("I5: confidence below τ_rung".into()),
            );
        }

        // ── ALLOW: mint the grant. TTL ≤ 30 s; two signatures for rung ≥
        // ANNOUNCE (the two-key model, spec §12.3).
        self.grant_seq += 1;
        let granted_autonomy = if capped {
            AutonomyLevel::HumanDirected
        } else {
            req.autonomy_requested
        };
        let authorized_by = operator_verified
            .clone()
            .unwrap_or_else(|| req.rule_id.clone());
        let mut g = ActionGrant {
            grant_id: format!("grant-{:06}", self.grant_seq),
            mission_id: req.mission_id.clone(),
            asset_id: req.asset_id.clone(),
            rung: req.rung,
            autonomy: granted_autonomy,
            geofence_hash: geofence_hash(&req.geofence),
            not_before: req.at,
            not_after: req.at + chrono::Duration::seconds(caps::GRANT_TTL_SECONDS),
            authorized_by,
            rule_version: req.rule_id.clone(),
            policy_version: self.policy.version.clone(),
            governor_signature: Vec::new(),
            operator_signature: Vec::new(),
        };
        if req.rung.requires_human_signature() {
            // I1 above guarantees a verified signature exists on this path.
            g.operator_signature = req.operator_signature.clone().unwrap_or_default();
        }
        g.governor_signature = self.key.sign(&g.signing_bytes());

        self.finish(decision_id, req, Outcome::Allow(g), None)
    }

    fn finish(
        &mut self,
        decision_id: String,
        req: &AuthorizeRequest,
        outcome: Outcome,
        invariant: Option<String>,
    ) -> Decision {
        let actor = match &outcome {
            Outcome::Allow(g) => g.authorized_by.clone(),
            _ => req
                .operator_credential_id
                .clone()
                .unwrap_or_else(|| req.rule_id.clone()),
        };
        let action = format!(
            "authorize:{}:{}{}",
            req.rung.name(),
            outcome.name(),
            invariant
                .as_deref()
                .map(|i| format!(":{i}"))
                .unwrap_or_default()
        );
        let audit_record_id = self.audit_append(
            &req.mission_id,
            &actor,
            &action,
            &self.policy.version.clone(),
            req.entity_confidence,
            req.at,
        );
        Decision {
            decision_id,
            outcome,
            invariant_violated: invariant,
            audit_record_id,
        }
    }

    fn audit_append(
        &mut self,
        subject: &str,
        actor: &str,
        action: &str,
        rule_version: &str,
        confidence: f32,
        at: Timestamp,
    ) -> String {
        self.audit_seq += 1;
        self.audit.append(audit::record(
            self.audit_seq,
            subject,
            actor,
            action,
            rule_version,
            confidence,
            at,
        ))
    }
}
