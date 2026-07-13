//! missions — playbook evaluation, mission lifecycle, asset tasking.
//! The ONLY client of the Governor (spec §9.7).
//!
//! Failure mode is the whole point: Governor unreachable → NO physical
//! actions, the mission stays in OBSERVING, and the operator is alerted
//! loudly (G-10). Fail closed. There is no code path in this crate that
//! emits an asset command without a grant.

pub mod playbook;

use aegis_common::clock::Timestamp;
use aegis_common::types::{AssetState, AutonomyLevel, EscalationRung, Posture};
use governor::grant::ActionGrant;
use governor::{AuthorizeRequest, Decision, Outcome};
use playbook::{EntityFacts, Playbook};
use thiserror::Error;

/// The Governor as `missions` sees it: possibly unreachable. The direct
/// in-process wrapper is below; over the wire this is a gRPC client with a
/// deadline, and a timeout is `Unreachable` — never a retry-until-allow.
pub trait Authorizer {
    fn authorize(&mut self, req: &AuthorizeRequest) -> Result<Decision, GovernorUnreachable>;
}

#[derive(Debug, Error, PartialEq)]
#[error("governor unreachable")]
pub struct GovernorUnreachable;

pub struct DirectAuthorizer<'a>(pub &'a mut governor::Governor);

impl Authorizer for DirectAuthorizer<'_> {
    fn authorize(&mut self, req: &AuthorizeRequest) -> Result<Decision, GovernorUnreachable> {
        Ok(self.0.authorize(req))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionState {
    Opened,
    Observing,
    Engaged,
    HandedOff,
    Closed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    Proposed,
    AwaitingApproval,
    /// The supervised-launch abort window (8 s in the shipped playbook).
    CountingDown,
    Executing,
    Complete,
    AbortedByOperator,
    Denied,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub id: String,
    pub mission_id: String,
    pub rung: EscalationRung,
    pub autonomy: AutonomyLevel,
    pub asset_id: String,
    pub state: ActionState,
    pub abort_window_seconds: f64,
    pub execute_at: Option<Timestamp>,
    pub grant: Option<ActionGrant>,
    pub decision_id: String,
    pub authorized_by: String,
    pub abort_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Mission {
    pub id: String,
    pub alert_id: String,
    pub playbook_name: String,
    pub objective: String,
    pub state: MissionState,
    pub asset_id: String,
    pub actions: Vec<Action>,
}

pub struct MissionEngine {
    playbooks: Vec<Playbook>,
    pub missions: Vec<Mission>,
    /// Loud, undismissable operator alerts (e.g. "GOVERNOR UNREACHABLE").
    pub operator_alerts: Vec<String>,
    mission_seq: u64,
    action_seq: u64,
}

impl MissionEngine {
    pub fn new(playbooks: Vec<Playbook>) -> Self {
        Self {
            playbooks,
            missions: Vec::new(),
            operator_alerts: Vec::new(),
            mission_seq: 0,
            action_seq: 0,
        }
    }

    pub fn playbooks(&self) -> &[Playbook] {
        &self.playbooks
    }

    /// Which playbook (if any) fires for these facts. First match wins;
    /// playbook authors keep triggers mutually exclusive per property.
    pub fn matching_playbook(&self, facts: &EntityFacts, posture: Posture) -> Option<&Playbook> {
        self.playbooks
            .iter()
            .find(|pb| playbook::trigger_fires(pb, facts, posture).unwrap_or(false))
    }

    /// Open a mission for an alert and propose the playbook's FIRST action
    /// through the Governor. Later rungs are operator-initiated.
    #[allow(clippy::too_many_arguments)]
    pub fn open_mission(
        &mut self,
        authorizer: &mut dyn Authorizer,
        playbook_name: &str,
        alert_id: &str,
        asset_id: &str,
        asset_state: AssetState,
        facts: &EntityFacts,
        posture: Posture,
        envelope: aegis_common::geometry::Polygon,
        geofence: aegis_common::geometry::Polygon,
        zone_id: &str,
        at: Timestamp,
    ) -> String {
        self.mission_seq += 1;
        let mission_id = format!("msn-{:06}", self.mission_seq);
        let pb = self
            .playbooks
            .iter()
            .find(|p| p.name == playbook_name)
            .cloned()
            .expect("open_mission called with an unknown playbook");
        let mut mission = Mission {
            id: mission_id.clone(),
            alert_id: alert_id.to_string(),
            playbook_name: pb.name.clone(),
            objective: pb.description.trim().to_string(),
            state: MissionState::Opened,
            asset_id: asset_id.to_string(),
            actions: Vec::new(),
        };

        let first = pb
            .actions
            .first()
            .expect("playbook-lint guarantees at least one action");
        let rung = first.rung().expect("validated at load");
        let autonomy = first.autonomy().expect("validated at load");
        let abort_window = first.abort_window_seconds.unwrap_or(0.0);

        self.action_seq += 1;
        let action_id = format!("act-{:06}", self.action_seq);
        let req = AuthorizeRequest {
            mission_id: mission_id.clone(),
            rung,
            autonomy_requested: autonomy,
            asset_id: asset_id.to_string(),
            asset_state,
            zone_id: zone_id.to_string(),
            trajectory_envelope: envelope,
            geofence,
            entity_confidence: facts.confidence as f32,
            posture,
            at,
            operator_credential_id: None,
            operator_signature: None,
            policy_version: String::new(),
            rule_id: format!(
                "playbook:{}:v{}:{}",
                pb.name,
                pb.version,
                rung.name().to_lowercase()
            ),
        };

        let action = match authorizer.authorize(&req) {
            Err(GovernorUnreachable) => {
                // G-10: no grant, no command, observe and page the operator.
                mission.state = MissionState::Observing;
                self.operator_alerts.push(format!(
                    "GOVERNOR UNREACHABLE — no physical actions. Mission {mission_id} holding in OBSERVING."
                ));
                Action {
                    id: action_id,
                    mission_id: mission_id.clone(),
                    rung,
                    autonomy,
                    asset_id: asset_id.to_string(),
                    state: ActionState::Proposed,
                    abort_window_seconds: abort_window,
                    execute_at: None,
                    grant: None,
                    decision_id: String::new(),
                    authorized_by: String::new(),
                    abort_reason: None,
                }
            }
            Ok(decision) => match decision.outcome {
                Outcome::Allow(grant) => {
                    mission.state = MissionState::Observing;
                    Action {
                        id: action_id,
                        mission_id: mission_id.clone(),
                        rung,
                        autonomy,
                        asset_id: asset_id.to_string(),
                        state: ActionState::CountingDown,
                        abort_window_seconds: abort_window,
                        execute_at: Some(
                            at + chrono::Duration::microseconds((abort_window * 1e6) as i64),
                        ),
                        authorized_by: grant.authorized_by.clone(),
                        grant: Some(grant),
                        decision_id: decision.decision_id,
                        abort_reason: None,
                    }
                }
                Outcome::RequireApproval { .. } => {
                    mission.state = MissionState::Observing;
                    Action {
                        id: action_id,
                        mission_id: mission_id.clone(),
                        rung,
                        autonomy,
                        asset_id: asset_id.to_string(),
                        state: ActionState::AwaitingApproval,
                        abort_window_seconds: abort_window,
                        execute_at: None,
                        grant: None,
                        decision_id: decision.decision_id,
                        authorized_by: String::new(),
                        abort_reason: None,
                    }
                }
                Outcome::Deny { reason } => {
                    mission.state = MissionState::Observing;
                    Action {
                        id: action_id,
                        mission_id: mission_id.clone(),
                        rung,
                        autonomy,
                        asset_id: asset_id.to_string(),
                        state: ActionState::Denied,
                        abort_window_seconds: abort_window,
                        execute_at: None,
                        grant: None,
                        decision_id: decision.decision_id,
                        authorized_by: String::new(),
                        abort_reason: Some(reason),
                    }
                }
            },
        };
        mission.actions.push(action);
        self.missions.push(mission);
        mission_id
    }

    /// Operator-initiated rung (ANNOUNCE and above arrive here, carrying the
    /// operator signature from hold-to-authorize).
    #[allow(clippy::too_many_arguments)]
    pub fn request_rung(
        &mut self,
        authorizer: &mut dyn Authorizer,
        mission_id: &str,
        rung: EscalationRung,
        asset_id: &str,
        asset_state: AssetState,
        confidence: f32,
        posture: Posture,
        envelope: aegis_common::geometry::Polygon,
        geofence: aegis_common::geometry::Polygon,
        zone_id: &str,
        at: Timestamp,
        operator_credential_id: Option<String>,
        operator_signature: Option<Vec<u8>>,
    ) -> Result<Decision, GovernorUnreachable> {
        let req = AuthorizeRequest {
            mission_id: mission_id.to_string(),
            rung,
            autonomy_requested: AutonomyLevel::HumanDirected,
            asset_id: asset_id.to_string(),
            asset_state,
            zone_id: zone_id.to_string(),
            trajectory_envelope: envelope,
            geofence,
            entity_confidence: confidence,
            posture,
            at,
            operator_credential_id,
            operator_signature,
            policy_version: String::new(),
            rule_id: format!("operator-request:{}", rung.name().to_lowercase()),
        };
        let decision = authorizer.authorize(&req);
        if let Ok(d) = &decision {
            self.action_seq += 1;
            let action = Action {
                id: format!("act-{:06}", self.action_seq),
                mission_id: mission_id.to_string(),
                rung,
                autonomy: AutonomyLevel::HumanDirected,
                asset_id: asset_id.to_string(),
                state: match &d.outcome {
                    Outcome::Allow(_) => ActionState::Executing,
                    Outcome::RequireApproval { .. } => ActionState::AwaitingApproval,
                    Outcome::Deny { .. } => ActionState::Denied,
                },
                abort_window_seconds: 0.0,
                execute_at: (matches!(d.outcome, Outcome::Allow(_))).then_some(at),
                grant: match &d.outcome {
                    Outcome::Allow(g) => Some(g.clone()),
                    _ => None,
                },
                decision_id: d.decision_id.clone(),
                authorized_by: match &d.outcome {
                    Outcome::Allow(g) => g.authorized_by.clone(),
                    _ => String::new(),
                },
                abort_reason: None,
            };
            if let Some(m) = self.missions.iter_mut().find(|m| m.id == mission_id) {
                if matches!(d.outcome, Outcome::Allow(_)) && rung >= EscalationRung::Announce {
                    m.state = MissionState::Engaged;
                }
                m.actions.push(action);
            }
        } else {
            self.operator_alerts.push(format!(
                "GOVERNOR UNREACHABLE — {} on {} not executed. Fail closed.",
                rung.name(),
                mission_id
            ));
        }
        decision
    }

    /// Advance countdowns: COUNTING_DOWN actions whose window has elapsed
    /// become EXECUTING. Returns actions that just became executable so the
    /// caller can hand their grants to the asset-adapter.
    pub fn tick(&mut self, at: Timestamp) -> Vec<Action> {
        let mut ready = Vec::new();
        for m in &mut self.missions {
            for a in &mut m.actions {
                if a.state == ActionState::CountingDown {
                    if let Some(t) = a.execute_at {
                        if at >= t {
                            a.state = ActionState::Executing;
                            ready.push(a.clone());
                        }
                    }
                }
            }
        }
        ready
    }

    /// One key, always available, no grant needed: abort.
    pub fn abort(&mut self, action_id: &str, reason: &str) -> bool {
        for m in &mut self.missions {
            for a in &mut m.actions {
                if a.id == action_id && a.state == ActionState::CountingDown {
                    a.state = ActionState::AbortedByOperator;
                    a.abort_reason = Some(reason.to_string());
                    a.grant = None; // the grant dies with the abort
                    return true;
                }
            }
        }
        false
    }

    pub fn mission(&self, id: &str) -> Option<&Mission> {
        self.missions.iter().find(|m| m.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_common::clock::parse_ts;
    use aegis_common::geometry::Polygon;
    use aegis_common::types::ObjectClass;
    use aegis_common::types::ZoneClass;

    pub(crate) fn fence() -> Polygon {
        Polygon::from_pairs(&[[15.0, 15.0], [785.0, 15.0], [785.0, 385.0], [15.0, 385.0]])
    }

    pub(crate) fn envelope() -> Polygon {
        Polygon::from_pairs(&[
            [420.0, 110.0],
            [450.0, 110.0],
            [450.0, 250.0],
            [420.0, 250.0],
        ])
    }

    pub(crate) fn facts() -> EntityFacts {
        EntityFacts {
            class: ObjectClass::Person,
            identity_known: false,
            zone_class: ZoneClass::Grounds,
            confidence: 0.99,
            expected: false,
            dwell_seconds: 0.0,
        }
    }

    pub(crate) fn load_shipped_playbook() -> Playbook {
        let yaml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../playbooks/perimeter_breach_night.yaml"
        ))
        .unwrap();
        playbook::parse(&yaml).unwrap()
    }

    #[test]
    fn abort_window_flow() {
        let key = aegis_common::crypto::KeyPair::from_seed_hex(
            "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7", // ARCHLINT-ALLOW: test-only
        )
        .unwrap();
        let mut gov = governor::Governor::new(key);
        let mut engine = MissionEngine::new(vec![load_shipped_playbook()]);
        let at = parse_ts("2026-03-14T03:11:45.5Z").unwrap();
        let mid = engine.open_mission(
            &mut DirectAuthorizer(&mut gov),
            "perimeter_breach_night",
            "alert-1",
            "BEE-01",
            AssetState::Docked,
            &facts(),
            Posture::Away,
            envelope(),
            fence(),
            "grounds_north",
            at,
        );
        let m = engine.mission(&mid).unwrap();
        assert_eq!(m.actions[0].state, ActionState::CountingDown);
        assert_eq!(m.actions[0].abort_window_seconds, 8.0);
        // Window not elapsed → nothing executes.
        assert!(engine.tick(at + chrono::Duration::seconds(7)).is_empty());
        // Window elapsed → the action executes with its grant.
        let ready = engine.tick(at + chrono::Duration::seconds(8));
        assert_eq!(ready.len(), 1);
        assert!(ready[0].grant.is_some());
    }

    #[test]
    fn abort_kills_the_grant() {
        let key = aegis_common::crypto::KeyPair::from_seed_hex(
            "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7", // ARCHLINT-ALLOW: test-only
        )
        .unwrap();
        let mut gov = governor::Governor::new(key);
        let mut engine = MissionEngine::new(vec![load_shipped_playbook()]);
        let at = parse_ts("2026-03-14T03:11:45.5Z").unwrap();
        let mid = engine.open_mission(
            &mut DirectAuthorizer(&mut gov),
            "perimeter_breach_night",
            "alert-1",
            "BEE-01",
            AssetState::Docked,
            &facts(),
            Posture::Away,
            envelope(),
            fence(),
            "grounds_north",
            at,
        );
        let action_id = engine.mission(&mid).unwrap().actions[0].id.clone();
        assert!(engine.abort(&action_id, "operator saw the housekeeper"));
        assert!(engine.tick(at + chrono::Duration::seconds(9)).is_empty());
        let a = &engine.mission(&mid).unwrap().actions[0];
        assert_eq!(a.state, ActionState::AbortedByOperator);
        assert!(a.grant.is_none());
    }
}
