//! Property-based tests over invariants I1–I9 (spec §12.4): randomly
//! generated authorize requests, and the postconditions that must hold on
//! every one of them. "This is not optional coverage; it is the reason the
//! product is insurable."

use aegis_common::clock::{parse_ts, Timestamp};
use aegis_common::crypto::KeyPair;
use aegis_common::geometry::{Point, Polygon};
use aegis_common::types::{AssetState, AutonomyLevel, EscalationRung, Posture};
use governor::grant::verify_grant;
use governor::{approval_message, caps, AuthorizeRequest, Governor, Outcome};
use proptest::prelude::*;

const GOVERNOR_SEED: &str = "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7";
const OPERATOR_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const FORGER_SEED: &str = "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb";

fn fence() -> Polygon {
    Polygon::from_pairs(&[[15.0, 15.0], [785.0, 15.0], [785.0, 385.0], [15.0, 385.0]])
}

#[derive(Debug, Clone, Copy)]
enum SigKind {
    None,
    Valid,
    Forged,
}

fn rung_strategy() -> impl Strategy<Value = EscalationRung> {
    prop::sample::select(EscalationRung::ALL.to_vec())
}

fn autonomy_strategy() -> impl Strategy<Value = AutonomyLevel> {
    prop::sample::select(vec![
        AutonomyLevel::HumanDirected,
        AutonomyLevel::HumanSupervised,
        AutonomyLevel::HumanOnLoop,
    ])
}

fn asset_state_strategy() -> impl Strategy<Value = AssetState> {
    prop::sample::select(vec![
        AssetState::Docked,
        AssetState::Launching,
        AssetState::Enroute,
        AssetState::OnStation,
        AssetState::Returning,
        AssetState::AssetFault,
    ])
}

fn posture_strategy() -> impl Strategy<Value = Posture> {
    prop::sample::select(vec![
        Posture::Nominal,
        Posture::Away,
        Posture::Night,
        Posture::Elevated,
        Posture::Lockdown,
    ])
}

fn sig_strategy() -> impl Strategy<Value = SigKind> {
    prop::sample::select(vec![SigKind::None, SigKind::Valid, SigKind::Forged])
}

/// Envelope generator: a rectangle whose centre and half-extent are random;
/// may or may not stay inside the fence. Degenerate (empty) sometimes.
fn envelope_strategy() -> impl Strategy<Value = Polygon> {
    (
        -100.0..900.0f64,
        -100.0..500.0f64,
        0.0..200.0f64,
        0.0..200.0f64,
        prop::bool::weighted(0.05),
    )
        .prop_map(|(cx, cy, hx, hy, empty)| {
            if empty {
                Polygon(vec![])
            } else {
                Polygon(vec![
                    Point::new(cx - hx, cy - hy),
                    Point::new(cx + hx, cy - hy),
                    Point::new(cx + hx, cy + hy),
                    Point::new(cx - hx, cy + hy),
                ])
            }
        })
}

fn ts() -> Timestamp {
    parse_ts("2026-03-14T03:11:45.500Z").unwrap()
}

#[derive(Debug, Clone)]
struct Scenario {
    rung: EscalationRung,
    autonomy: AutonomyLevel,
    asset_state: AssetState,
    posture: Posture,
    envelope: Polygon,
    confidence: f32,
    sig: SigKind,
    chain_broken: bool,
    clock_skew: bool,
    degraded: bool,
}

fn scenario_strategy() -> impl Strategy<Value = Scenario> {
    (
        rung_strategy(),
        autonomy_strategy(),
        asset_state_strategy(),
        posture_strategy(),
        envelope_strategy(),
        0.0..1.0f32,
        sig_strategy(),
        prop::bool::weighted(0.15),
        prop::bool::weighted(0.1),
        prop::bool::weighted(0.1),
    )
        .prop_map(
            |(
                rung,
                autonomy,
                asset_state,
                posture,
                envelope,
                confidence,
                sig,
                chain_broken,
                clock_skew,
                degraded,
            )| Scenario {
                rung,
                autonomy,
                asset_state,
                posture,
                envelope,
                confidence,
                sig,
                chain_broken,
                clock_skew,
                degraded,
            },
        )
}

fn run(s: &Scenario) -> (Governor, AuthorizeRequest, governor::Decision) {
    let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
    let op = KeyPair::from_seed_hex(OPERATOR_SEED).unwrap();
    let forger = KeyPair::from_seed_hex(FORGER_SEED).unwrap();
    gov.register_operator("MERIDIAN-2", op.public_key_bytes());
    gov.set_log_chain_broken(s.chain_broken);
    gov.set_clock_skew(s.clock_skew);
    gov.set_degraded(s.degraded);

    let mut req = AuthorizeRequest {
        mission_id: "msn-prop".into(),
        rung: s.rung,
        autonomy_requested: s.autonomy,
        asset_id: "BEE-01".into(),
        asset_state: s.asset_state,
        zone_id: "grounds_north".into(),
        trajectory_envelope: s.envelope.clone(),
        geofence: fence(),
        entity_confidence: s.confidence,
        posture: s.posture,
        at: ts(),
        operator_credential_id: None,
        operator_signature: None,
        policy_version: "compiled-default".into(),
        rule_id: "prop-rule".into(),
    };
    match s.sig {
        SigKind::None => {}
        SigKind::Valid => {
            req.operator_credential_id = Some("MERIDIAN-2".into());
            req.operator_signature =
                Some(op.sign(&approval_message(&req.mission_id, req.rung, req.at)));
        }
        SigKind::Forged => {
            req.operator_credential_id = Some("MERIDIAN-2".into());
            req.operator_signature =
                Some(forger.sign(&approval_message(&req.mission_id, req.rung, req.at)));
        }
    }
    let d = gov.authorize(&req);
    (gov, req, d)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// The postconditions of ALLOW — every invariant, on every random request.
    #[test]
    fn allow_implies_every_invariant(s in scenario_strategy()) {
        let (gov, req, d) = run(&s);
        if let Outcome::Allow(g) = &d.outcome {
            // I7/I8: never ALLOW in a fail-closed state.
            prop_assert!(!s.chain_broken && !s.clock_skew && !s.degraded);
            // I6: asset was taskable.
            prop_assert!(matches!(s.asset_state, AssetState::Docked | AssetState::OnStation));
            // I3: envelope wholly inside the fence.
            prop_assert!(req.geofence.contains_polygon(&req.trajectory_envelope));
            // I1: rung ≥ ANNOUNCE carries a verified operator signature (two keys).
            if s.rung.requires_human_signature() {
                prop_assert!(matches!(s.sig, SigKind::Valid));
                prop_assert_eq!(g.signature_count(), 2);
                prop_assert_eq!(g.authorized_by.as_str(), "MERIDIAN-2");
            }
            // I2/I4: granted autonomy never exceeds the compiled cap.
            prop_assert!(g.autonomy <= caps::compiled_max_autonomy(s.rung));
            // I5: weak signals only pass with a human.
            if s.confidence < caps::compiled_confidence_floor(s.rung) {
                prop_assert!(matches!(s.sig, SigKind::Valid));
            }
            // Grant hygiene: TTL ≤ 30 s and it verifies against the governor key.
            prop_assert!(g.not_after - g.not_before <= chrono::Duration::seconds(caps::GRANT_TTL_SECONDS));
            prop_assert!(verify_grant(g, &gov.public_key(), &g.geofence_hash, req.at).is_ok());
            // A2: authorized_by is an operator or a rule — never empty.
            prop_assert!(!g.authorized_by.is_empty());
        }
    }

    /// Fail-closed states deny everything physical, regardless of signatures.
    #[test]
    fn i7_i8_deny_all(mut s in scenario_strategy(), which in 0..3usize) {
        // Force at least one fail-closed flag; keep any others the generator set.
        match which {
            0 => s.chain_broken = true,
            1 => s.clock_skew = true,
            _ => s.degraded = true,
        }
        let (_, _, d) = run(&s);
        if matches!(s.sig, SigKind::Forged) {
            // Forgery is itself a DENY path; either way, never ALLOW.
            prop_assert!(!matches!(d.outcome, Outcome::Allow(_)));
        } else {
            let denied = matches!(d.outcome, Outcome::Deny { .. });
            prop_assert!(denied, "fail-closed state must DENY, got {}", d.outcome.name());
        }
    }

    /// A forged signature is ALWAYS denied and ALWAYS a security event (G-03).
    #[test]
    fn forged_signature_always_denied(mut s in scenario_strategy()) {
        s.sig = SigKind::Forged;
        let (gov, _, d) = run(&s);
        let denied = matches!(d.outcome, Outcome::Deny { .. });
        prop_assert!(denied, "forgery must DENY, got {}", d.outcome.name());
        prop_assert_eq!(gov.security_events().len(), 1);
        prop_assert_eq!(gov.security_events()[0].kind.as_str(), "forged_signature");
    }

    /// I1: without a valid signature, rung ≥ ANNOUNCE is never allowed.
    #[test]
    fn announce_and_above_never_allowed_without_human(
        mut s in scenario_strategy(),
        rung in prop::sample::select(vec![
            EscalationRung::Announce,
            EscalationRung::Shadow,
            EscalationRung::Deny,
            EscalationRung::Handoff,
        ]),
        no_sig in prop::bool::ANY,
    ) {
        s.rung = rung;
        s.sig = if no_sig { SigKind::None } else { SigKind::Forged };
        let (_, _, d) = run(&s);
        prop_assert!(!matches!(d.outcome, Outcome::Allow(_)));
    }

    /// An audit record exists after EVERY decision, and the chain verifies.
    #[test]
    fn every_path_is_audited(s in scenario_strategy()) {
        let (gov, _, d) = run(&s);
        prop_assert_eq!(gov.audit_records().len(), 1);
        prop_assert!(gov.audit_chain_intact());
        prop_assert!(!d.audit_record_id.is_empty());
    }

    /// I9: no integer outside 1..=6 deserialises into a rung.
    #[test]
    fn i9_no_seventh_rung(v in prop::num::i32::ANY) {
        prop_assume!(!(1..=6).contains(&v));
        prop_assert!(EscalationRung::try_from(v).is_err());
    }
}
