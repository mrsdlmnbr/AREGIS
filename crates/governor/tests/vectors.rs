//! Governor test vectors G-01…G-10 (spec §12.4) as literal tests.
//!
//! G-10 (governor unreachable ⟹ missions denies, observes, alerts) is a
//! behaviour of the `missions` crate and lives in
//! crates/missions/tests/g10_governor_unreachable.rs — the Governor cannot
//! test its own absence.

use aegis_common::clock::{parse_ts, Timestamp};
use aegis_common::crypto::KeyPair;
use aegis_common::geometry::Polygon;
use aegis_common::types::{AssetState, AutonomyLevel, EscalationRung, Posture};
use governor::grant::{verify_grant, GrantRejection};
use governor::policy::{Policy, PolicyRejection};
use governor::{approval_message, geofence_hash, AuthorizeRequest, Governor, Outcome};

const GOVERNOR_SEED: &str = "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7";
const OPERATOR_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const FORGER_SEED: &str = "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb";

fn fence() -> Polygon {
    Polygon::from_pairs(&[[15.0, 15.0], [785.0, 15.0], [785.0, 385.0], [15.0, 385.0]])
}

fn inside_envelope() -> Polygon {
    Polygon::from_pairs(&[
        [420.0, 110.0],
        [450.0, 110.0],
        [450.0, 250.0],
        [420.0, 250.0],
    ])
}

/// G-04's envelope: clips 2 m outside the east fence line at x = 785.
fn clipping_envelope() -> Polygon {
    Polygon::from_pairs(&[
        [700.0, 100.0],
        [787.0, 100.0],
        [787.0, 120.0],
        [700.0, 120.0],
    ])
}

fn at() -> Timestamp {
    parse_ts("2026-03-14T03:11:45.500Z").unwrap()
}

fn governor() -> (Governor, KeyPair) {
    let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
    let op = KeyPair::from_seed_hex(OPERATOR_SEED).unwrap();
    gov.register_operator("MERIDIAN-2", op.public_key_bytes());
    (gov, op)
}

fn base_request(rung: EscalationRung, autonomy: AutonomyLevel) -> AuthorizeRequest {
    AuthorizeRequest {
        mission_id: "msn-000001".into(),
        rung,
        autonomy_requested: autonomy,
        asset_id: "BEE-01".into(),
        asset_state: AssetState::Docked,
        zone_id: "grounds_north".into(),
        trajectory_envelope: inside_envelope(),
        geofence: fence(),
        entity_confidence: 0.91,
        posture: Posture::Away,
        at: at(),
        operator_credential_id: None,
        operator_signature: None,
        policy_version: "compiled-default".into(),
        rule_id: "playbook:perimeter_breach_night:v4:observe".into(),
    }
}

fn signed(mut req: AuthorizeRequest, op: &KeyPair, credential: &str) -> AuthorizeRequest {
    let msg = approval_message(&req.mission_id, req.rung, req.at);
    req.operator_credential_id = Some(credential.into());
    req.operator_signature = Some(op.sign(&msg));
    req
}

// ── G-01: OBSERVE, on-loop, conf 0.91, inside fence → ALLOW, 1 signature ────
#[test]
fn g01_observe_on_loop_allowed_with_one_signature() {
    let (mut gov, _) = governor();
    let d = gov.authorize(&base_request(
        EscalationRung::Observe,
        AutonomyLevel::HumanOnLoop,
    ));
    match d.outcome {
        Outcome::Allow(g) => {
            assert_eq!(
                g.signature_count(),
                1,
                "OBSERVE grant carries the governor key only"
            );
            assert_eq!(g.rung, EscalationRung::Observe);
            assert_eq!(g.autonomy, AutonomyLevel::HumanOnLoop);
            assert!(g.operator_signature.is_empty());
            assert!(verify_grant(&g, &gov.public_key(), &g.geofence_hash, at()).is_ok());
        }
        other => panic!("expected ALLOW, got {other:?}"),
    }
    assert!(d.invariant_violated.is_none());
    assert!(gov.audit_chain_intact());
    assert_eq!(
        gov.audit_records().len(),
        1,
        "ALLOW writes an audit record too"
    );
}

// ── G-02: ANNOUNCE, no operator signature → REQUIRE_APPROVAL ────────────────
#[test]
fn g02_announce_without_signature_requires_approval() {
    let (mut gov, _) = governor();
    let d = gov.authorize(&base_request(
        EscalationRung::Announce,
        AutonomyLevel::HumanDirected,
    ));
    assert!(matches!(d.outcome, Outcome::RequireApproval { .. }));
    assert_eq!(
        d.invariant_violated.as_deref(),
        Some("I1: operator signature required")
    );
    assert!(
        gov.security_events().is_empty(),
        "an absent signature is not an attack"
    );
}

// ── G-03: ANNOUNCE, forged operator signature → DENY + P0 security event ────
#[test]
fn g03_forged_signature_is_denied_and_paged() {
    let (mut gov, _) = governor();
    let forger = KeyPair::from_seed_hex(FORGER_SEED).unwrap();
    let req = signed(
        base_request(EscalationRung::Announce, AutonomyLevel::HumanDirected),
        &forger,
        "MERIDIAN-2", // claims a real credential, signs with the wrong key
    );
    let d = gov.authorize(&req);
    assert!(
        matches!(d.outcome, Outcome::Deny { .. }),
        "forgery is DENY, never a retry prompt"
    );
    assert_eq!(gov.security_events().len(), 1);
    assert_eq!(gov.security_events()[0].kind, "forged_signature");
    assert!(gov.audit_chain_intact());
}

#[test]
fn g03b_unregistered_credential_is_denied_and_paged() {
    let (mut gov, _) = governor();
    let forger = KeyPair::from_seed_hex(FORGER_SEED).unwrap();
    let req = signed(
        base_request(EscalationRung::Announce, AutonomyLevel::HumanDirected),
        &forger,
        "NOBODY-9",
    );
    let d = gov.authorize(&req);
    assert!(matches!(d.outcome, Outcome::Deny { .. }));
    assert_eq!(gov.security_events()[0].kind, "forged_signature");
}

// ── G-04: OBSERVE, trajectory clips 2 m outside fence → DENY ────────────────
#[test]
fn g04_trajectory_clipping_fence_is_denied() {
    let (mut gov, _) = governor();
    let mut req = base_request(EscalationRung::Observe, AutonomyLevel::HumanOnLoop);
    req.trajectory_envelope = clipping_envelope();
    let d = gov.authorize(&req);
    assert!(matches!(d.outcome, Outcome::Deny { .. }));
    assert_eq!(
        d.invariant_violated.as_deref(),
        Some("I3: trajectory leaves geofence")
    );
}

#[test]
fn g04b_empty_envelope_fails_closed() {
    let (mut gov, _) = governor();
    let mut req = base_request(EscalationRung::Observe, AutonomyLevel::HumanOnLoop);
    req.trajectory_envelope = Polygon(vec![]);
    let d = gov.authorize(&req);
    assert!(matches!(d.outcome, Outcome::Deny { .. }));
    assert_eq!(
        d.invariant_violated.as_deref(),
        Some("I3: trajectory leaves geofence")
    );
}

// ── G-05: policy requesting HUMAN_ON_LOOP for ANNOUNCE → rejected at load ───
#[test]
fn g05_loosening_policy_rejected_at_load_last_good_kept() {
    let (mut gov, _) = governor();
    let good = Policy {
        version: "prop-1.first-good".into(),
        jurisdiction: "US-CO".into(),
        max_autonomy_by_rung: Default::default(),
        min_confidence_by_rung: Default::default(),
    };
    gov.load_policy(good, at()).unwrap();
    assert_eq!(gov.policy_version(), "prop-1.first-good");

    let mut loosening = Policy {
        version: "prop-2.loosened".into(),
        jurisdiction: "US-CO".into(),
        max_autonomy_by_rung: Default::default(),
        min_confidence_by_rung: Default::default(),
    };
    loosening
        .max_autonomy_by_rung
        .insert(EscalationRung::Announce, AutonomyLevel::HumanOnLoop);
    let err = gov.load_policy(loosening, at()).unwrap_err();
    assert!(matches!(err, PolicyRejection::LoosensAutonomy { .. }));
    assert_eq!(
        gov.policy_version(),
        "prop-1.first-good",
        "governor runs on last-good policy"
    );

    // And even if such a policy somehow took effect, the compiled cap would
    // still bind: an autonomous ANNOUNCE request cannot be allowed.
    let mut req = base_request(EscalationRung::Announce, AutonomyLevel::HumanOnLoop);
    req.entity_confidence = 0.99;
    let d = gov.authorize(&req);
    assert!(!matches!(d.outcome, Outcome::Allow(_)));
}

// ── G-06: log-chain break → all physical DENY until a human clears ──────────
#[test]
fn g06_chain_break_denies_everything_until_cleared() {
    let (mut gov, op) = governor();
    gov.set_log_chain_broken(true);
    for rung in EscalationRung::ALL {
        let req = signed(
            base_request(rung, AutonomyLevel::HumanDirected),
            &op,
            "MERIDIAN-2",
        );
        let d = gov.authorize(&req);
        assert!(
            matches!(d.outcome, Outcome::Deny { .. }),
            "{rung:?} must be denied during a chain break — even human-directed"
        );
        assert_eq!(
            d.invariant_violated.as_deref(),
            Some("I7: log-chain break or clock skew")
        );
    }
    gov.clear_log_chain_break("MERIDIAN-2", at());
    let d = gov.authorize(&base_request(
        EscalationRung::Observe,
        AutonomyLevel::HumanOnLoop,
    ));
    assert!(
        matches!(d.outcome, Outcome::Allow(_)),
        "clearing restores service"
    );
    assert!(gov.audit_chain_intact());
}

// ── G-07: grant replayed 31 s later → asset rejects (TTL) ───────────────────
#[test]
fn g07_replayed_grant_is_dead() {
    let (mut gov, _) = governor();
    let d = gov.authorize(&base_request(
        EscalationRung::Observe,
        AutonomyLevel::HumanOnLoop,
    ));
    let g = match d.outcome {
        Outcome::Allow(g) => g,
        other => panic!("expected ALLOW, got {other:?}"),
    };
    let replay_at = at() + chrono::Duration::seconds(31);
    assert_eq!(
        verify_grant(&g, &gov.public_key(), &g.geofence_hash, replay_at),
        Err(GrantRejection::Expired)
    );
    // At 29 s it is still alive — the TTL is the boundary, not luck.
    assert!(verify_grant(
        &g,
        &gov.public_key(),
        &g.geofence_hash,
        at() + chrono::Duration::seconds(29)
    )
    .is_ok());
}

// ── G-08: grant with mismatched geofence_hash → asset rejects ───────────────
#[test]
fn g08_geofence_hash_mismatch_is_rejected() {
    let (mut gov, _) = governor();
    let d = gov.authorize(&base_request(
        EscalationRung::Observe,
        AutonomyLevel::HumanOnLoop,
    ));
    let g = match d.outcome {
        Outcome::Allow(g) => g,
        other => panic!("expected ALLOW, got {other:?}"),
    };
    let other_fence = Polygon::from_pairs(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    let loaded = geofence_hash(&other_fence);
    assert_eq!(
        verify_grant(&g, &gov.public_key(), &loaded, at()),
        Err(GrantRejection::GeofenceMismatch)
    );
}

// ── G-08b: a tampered grant fails the governor signature ────────────────────
#[test]
fn g08b_tampered_grant_fails_signature() {
    let (mut gov, _) = governor();
    let d = gov.authorize(&base_request(
        EscalationRung::Observe,
        AutonomyLevel::HumanOnLoop,
    ));
    let g = match d.outcome {
        Outcome::Allow(g) => g,
        other => panic!("expected ALLOW, got {other:?}"),
    };
    // Redirect the grant to a different asset: signature no longer covers it.
    let mut retargeted = g.clone();
    retargeted.asset_id = "ARGUS-1".into();
    assert_eq!(
        verify_grant(
            &retargeted,
            &gov.public_key(),
            &retargeted.geofence_hash,
            at()
        ),
        Err(GrantRejection::BadGovernorSignature)
    );
    // Escalate the rung inside a signed grant: refused too (the two-key check
    // fires first for rung ≥ ANNOUNCE — either way, a tampered grant is dead).
    let mut escalated = g.clone();
    escalated.rung = EscalationRung::Shadow;
    assert!(verify_grant(
        &escalated,
        &gov.public_key(),
        &escalated.geofence_hash,
        at()
    )
    .is_err());
    let mut escalated_low = g;
    escalated_low.rung = EscalationRung::Illuminate;
    assert_eq!(
        verify_grant(
            &escalated_low,
            &gov.public_key(),
            &escalated_low.geofence_hash,
            at()
        ),
        Err(GrantRejection::BadGovernorSignature)
    );
}

// ── G-09: any rung value above HANDOFF fails to deserialise ─────────────────
#[test]
fn g09_rung_seven_does_not_exist() {
    assert!(EscalationRung::try_from(7).is_err());
    assert!(serde_json::from_str::<EscalationRung>("\"FORCE\"").is_err());
    assert!(serde_json::from_str::<AuthorizeRequest>(
        &serde_json::to_string(&serde_json::json!({
            "mission_id": "m", "rung": "RUNG_7", "autonomy_requested": "HUMAN_DIRECTED",
            "asset_id": "a", "asset_state": "DOCKED", "zone_id": "z",
            "trajectory_envelope": [], "geofence": [], "entity_confidence": 1.0,
            "posture": "AWAY", "at": "2026-03-14T03:11:45Z",
            "operator_credential_id": null, "operator_signature": null,
            "policy_version": "v", "rule_id": "r"
        }))
        .unwrap()
    )
    .is_err());
}

// ── two-key model: a valid ANNOUNCE grant carries both signatures ───────────
#[test]
fn announce_with_valid_signature_allows_with_two_keys() {
    let (mut gov, op) = governor();
    let mut req = signed(
        base_request(EscalationRung::Announce, AutonomyLevel::HumanDirected),
        &op,
        "MERIDIAN-2",
    );
    req.entity_confidence = 0.95;
    let d = gov.authorize(&req);
    match d.outcome {
        Outcome::Allow(g) => {
            assert_eq!(
                g.signature_count(),
                2,
                "governor + operator: the two-key model"
            );
            assert_eq!(g.authorized_by, "MERIDIAN-2");
            assert!(verify_grant(&g, &gov.public_key(), &g.geofence_hash, at()).is_ok());
        }
        other => panic!("expected ALLOW, got {other:?}"),
    }
}

// ── I5: weak signal never auto-acts ─────────────────────────────────────────
#[test]
fn i5_low_confidence_requires_approval() {
    let (mut gov, _) = governor();
    let mut req = base_request(EscalationRung::Observe, AutonomyLevel::HumanSupervised);
    req.entity_confidence = 0.60;
    let d = gov.authorize(&req);
    assert!(matches!(d.outcome, Outcome::RequireApproval { .. }));
    assert_eq!(
        d.invariant_violated.as_deref(),
        Some("I5: confidence below τ_rung")
    );
}

// ── I6: a faulted asset is never tasked ─────────────────────────────────────
#[test]
fn i6_faulted_asset_is_denied() {
    let (mut gov, _) = governor();
    let mut req = base_request(EscalationRung::Observe, AutonomyLevel::HumanOnLoop);
    req.asset_state = AssetState::AssetFault;
    let d = gov.authorize(&req);
    assert!(matches!(d.outcome, Outcome::Deny { .. }));
    assert_eq!(
        d.invariant_violated.as_deref(),
        Some("I6: asset not READY/ON_STATION")
    );
}
