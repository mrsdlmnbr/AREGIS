//! G-10 (spec §12.4): Governor unreachable → `missions` denies, observes,
//! alerts the operator. Fail closed. This is the vector the Governor cannot
//! host: the test of its absence.

use aegis_common::clock::parse_ts;
use aegis_common::geometry::Polygon;
use aegis_common::types::{AssetState, ObjectClass, Posture, ZoneClass};
use governor::{AuthorizeRequest, Decision};
use missions::playbook::{self, EntityFacts};
use missions::{ActionState, Authorizer, GovernorUnreachable, MissionEngine, MissionState};

/// A dead Governor: every call fails. The engine must treat this exactly as
/// doctrine demands — no grant, no command, loud alert.
struct DeadGovernor;

impl Authorizer for DeadGovernor {
    fn authorize(&mut self, _req: &AuthorizeRequest) -> Result<Decision, GovernorUnreachable> {
        Err(GovernorUnreachable)
    }
}

#[test]
fn g10_governor_unreachable_fails_closed() {
    let yaml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../playbooks/perimeter_breach_night.yaml"
    ))
    .unwrap();
    let pb = playbook::parse(&yaml).unwrap();
    let mut engine = MissionEngine::new(vec![pb]);

    let facts = EntityFacts {
        class: ObjectClass::Person,
        identity_known: false,
        zone_class: ZoneClass::Grounds,
        confidence: 0.99,
        expected: false,
        dwell_seconds: 0.0,
    };
    let at = parse_ts("2026-03-14T03:11:45.5Z").unwrap();
    let fence = Polygon::from_pairs(&[[15.0, 15.0], [785.0, 15.0], [785.0, 385.0], [15.0, 385.0]]);
    let envelope =
        Polygon::from_pairs(&[[420.0, 110.0], [450.0, 110.0], [450.0, 250.0], [420.0, 250.0]]);

    let mid = engine.open_mission(
        &mut DeadGovernor,
        "perimeter_breach_night",
        "alert-1",
        "BEE-01",
        AssetState::Docked,
        &facts,
        Posture::Away,
        envelope,
        fence,
        "grounds_north",
        at,
    );

    let m = engine.mission(&mid).unwrap();
    // The mission exists and observes — we never go blind…
    assert_eq!(m.state, MissionState::Observing);
    // …but nothing physical happened: no grant, no countdown, no execution.
    assert_eq!(m.actions[0].state, ActionState::Proposed);
    assert!(m.actions[0].grant.is_none());
    assert!(engine.tick(at + chrono::Duration::seconds(60)).is_empty());
    // And the operator was alerted loudly.
    assert_eq!(engine.operator_alerts.len(), 1);
    assert!(engine.operator_alerts[0].contains("GOVERNOR UNREACHABLE"));
}
