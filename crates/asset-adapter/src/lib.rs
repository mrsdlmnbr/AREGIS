//! asset-adapter — grant verification + the mission→robot protocol (spec §13).
//!
//! `missions` never talks to a robot; it talks to this adapter, and only with
//! an ActionGrant. The adapter verifies the grant, translates the rung into a
//! mission-level verb (`goto · orbit · observe · follow · return · recall` —
//! never a joystick stream), and the asset verifies the grant AGAIN onboard
//! against the Governor public key it holds. Third enforcement point: the
//! onboard watchdog, which cannot be commanded off.
//!
//! `recall` requires no grant. Stopping is always allowed — every dangerous
//! verb needs a key; the safe verb needs none.

use aegis_common::clock::Timestamp;
use aegis_common::geometry::{Point, Polygon};
use aegis_common::types::{AssetKind, AssetState, EscalationRung};
use governor::grant::{verify_grant, ActionGrant, GrantRejection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TaskRejection {
    #[error("grant rejected onboard: {0}")]
    Grant(#[from] GrantRejection),
    #[error("rung {0:?} not in this asset's permitted set")]
    RungNotPermitted(EscalationRung),
    #[error("asset is faulted")]
    Faulted,
}

/// Independent onboard watchdog (spec §13): forces return-to-dock on fence
/// breach, link loss > 5 s, low battery, or grant expiry. Separate from the
/// "flight stack" (the movement code below) and not commandable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watchdog {
    pub link_loss_limit_s: f64,
    pub battery_floor: f32,
    link_lost_since: Option<Timestamp>,
}

impl Default for Watchdog {
    fn default() -> Self {
        Self { link_loss_limit_s: 5.0, battery_floor: 0.15, link_lost_since: None }
    }
}

/// A simulated asset with the onboard half of the safety model: its own copy
/// of the geofence, the Governor's public key, and the watchdog. The SITL
/// harness drives this; the real firmware implements the same contract
/// (tools/dal-conformance is its acceptance test).
#[derive(Debug, Clone)]
pub struct SimAsset {
    pub id: String,
    pub kind: AssetKind,
    pub state: AssetState,
    pub battery: f32,
    pub position: Point,
    pub dock: Point,
    pub speed_mps: f64,
    pub launch_seconds: f64,
    pub permitted_rungs: Vec<EscalationRung>,
    /// Held ONBOARD. The grant's geofence_hash must match the hash of this.
    pub onboard_geofence: Polygon,
    pub geofence_hash: Vec<u8>,
    pub governor_pubkey: [u8; 32],
    pub watchdog: Watchdog,

    /// True while the asset is holding at the fence line instead of
    /// following a target beyond it.
    pub geofence_hold: bool,
    /// Optics masked in the encoder when the field of view crosses the
    /// property line (spec §3.3) — the masked frames are the recorded frames.
    pub optics_masked: bool,
    pub announcements_made: u32,
    pub geofence_breaches: u32,

    target: Option<Point>,
    launch_remaining: f64,
    active_grant_expiry: Option<Timestamp>,
}

impl SimAsset {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        kind: AssetKind,
        dock: Point,
        speed_mps: f64,
        launch_seconds: f64,
        permitted_rungs: Vec<EscalationRung>,
        onboard_geofence: Polygon,
        governor_pubkey: [u8; 32],
    ) -> Self {
        let geofence_hash = governor::geofence_hash(&onboard_geofence);
        Self {
            id: id.to_string(),
            kind,
            state: AssetState::Docked,
            battery: 1.0,
            position: dock,
            dock,
            speed_mps,
            launch_seconds,
            permitted_rungs,
            onboard_geofence,
            geofence_hash,
            governor_pubkey,
            watchdog: Watchdog::default(),
            geofence_hold: false,
            optics_masked: false,
            announcements_made: 0,
            geofence_breaches: 0,
            target: None,
            launch_remaining: 0.0,
            active_grant_expiry: None,
        }
    }

    /// The onboard gate (spec §13): verify the grant AGAIN with the key and
    /// fence THIS asset holds. A compromised adapter cannot help a bad grant
    /// past this point.
    pub fn verify_onboard(&self, grant: &ActionGrant, now: Timestamp) -> Result<(), TaskRejection> {
        if self.state == AssetState::AssetFault {
            return Err(TaskRejection::Faulted);
        }
        if !self.permitted_rungs.contains(&grant.rung) {
            return Err(TaskRejection::RungNotPermitted(grant.rung));
        }
        verify_grant(grant, &self.governor_pubkey, &self.geofence_hash, now)?;
        Ok(())
    }

    /// Task the asset toward a station point. Only callable with a verified
    /// grant (the adapter enforces it; the asset re-verifies).
    pub fn task_observe(&mut self, grant: &ActionGrant, station: Point, now: Timestamp) -> Result<(), TaskRejection> {
        self.verify_onboard(grant, now)?;
        self.active_grant_expiry = Some(grant.not_after);
        // Clip the station to the fence: the asset never even AIMS outside.
        self.target = Some(self.clip_to_fence(station));
        if self.state == AssetState::Docked {
            self.state = AssetState::Launching;
            self.launch_remaining = self.launch_seconds;
        }
        Ok(())
    }

    /// ANNOUNCE through the asset's annunciator. Two-key verified onboard:
    /// this call is the reason G-03 matters.
    pub fn task_announce(&mut self, grant: &ActionGrant, now: Timestamp) -> Result<(), TaskRejection> {
        self.verify_onboard(grant, now)?;
        if grant.rung != EscalationRung::Announce {
            return Err(TaskRejection::RungNotPermitted(grant.rung));
        }
        self.announcements_made += 1;
        Ok(())
    }

    /// Recall requires NO grant. Stopping is always allowed.
    pub fn recall(&mut self) {
        self.target = None;
        self.active_grant_expiry = None;
        if self.state != AssetState::Docked && self.state != AssetState::AssetFault {
            self.state = AssetState::Returning;
        }
    }

    pub fn set_link_lost(&mut self, since: Option<Timestamp>) {
        self.watchdog.link_lost_since = since;
    }

    /// Advance the simulation by `dt` seconds at time `now`. All safety
    /// behaviour lives here: fence hold, watchdog, masking.
    pub fn step(&mut self, dt: f64, now: Timestamp) {
        // ── watchdog first: it outranks the flight stack.
        let watchdog_fired = self.battery < self.watchdog.battery_floor
            || self
                .watchdog
                .link_lost_since
                .map(|t| (now - t).num_milliseconds() as f64 / 1000.0 > self.watchdog.link_loss_limit_s)
                .unwrap_or(false)
            || self
                .active_grant_expiry
                .map(|exp| now > exp && self.state == AssetState::Enroute)
                .unwrap_or(false);
        if watchdog_fired && matches!(self.state, AssetState::Enroute | AssetState::OnStation | AssetState::Launching) {
            self.recall();
        }

        match self.state {
            AssetState::Launching => {
                self.launch_remaining -= dt;
                if self.launch_remaining <= 0.0 {
                    self.state = AssetState::Enroute;
                }
            }
            AssetState::Enroute | AssetState::OnStation => {
                if let Some(target) = self.target {
                    self.move_toward(target, dt);
                    if self.position.dist(&target) < 1.0 {
                        self.state = AssetState::OnStation;
                    } else if self.state == AssetState::OnStation {
                        self.state = AssetState::Enroute;
                    }
                }
            }
            AssetState::Returning => {
                let dock = self.dock;
                self.move_toward(dock, dt);
                if self.position.dist(&dock) < 1.0 {
                    self.state = AssetState::Docked;
                    self.geofence_hold = false;
                    self.optics_masked = false;
                }
            }
            AssetState::Docked | AssetState::AssetFault => {}
        }

        // The line the lawyers care about: the asset must never be outside.
        if !self.onboard_geofence.contains(&self.position) {
            self.geofence_breaches += 1;
            self.recall();
        }
    }

    /// Re-aim at a (possibly moving) target, holding at the fence if the
    /// target is beyond it. Requires an in-force grant already verified; the
    /// harness calls this only inside an OBSERVE engagement.
    pub fn track_target(&mut self, target: Point) {
        let clipped = self.clip_to_fence(target);
        let beyond_fence = !self.onboard_geofence.contains(&target);
        self.geofence_hold = beyond_fence;
        // FOV crosses the property line → mask in the encoder (spec §3.3).
        self.optics_masked = beyond_fence;
        self.target = Some(clipped);
    }

    fn clip_to_fence(&self, target: Point) -> Point {
        if self.onboard_geofence.contains(&target) {
            return target;
        }
        match self.onboard_geofence.first_boundary_crossing(&self.position, &target) {
            Some(t) => {
                // Hold 2 m inside the line, never on it.
                let t_hold = (t - 2.0 / self.position.dist(&target).max(1e-9)).max(0.0);
                Point::new(
                    self.position.x + t_hold * (target.x - self.position.x),
                    self.position.y + t_hold * (target.y - self.position.y),
                )
            }
            None => self.position, // no path inside → stay where we are
        }
    }

    fn move_toward(&mut self, target: Point, dt: f64) {
        let d = self.position.dist(&target);
        let step = self.speed_mps * dt;
        if d <= step {
            self.position = target;
        } else {
            self.position = Point::new(
                self.position.x + (target.x - self.position.x) / d * step,
                self.position.y + (target.y - self.position.y) / d * step,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_common::clock::parse_ts;
    use aegis_common::crypto::KeyPair;
    use aegis_common::types::{AutonomyLevel, Posture};
    use governor::{AuthorizeRequest, Governor, Outcome};

    const GOVERNOR_SEED: &str = "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7";
    const FORGER_SEED: &str = "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb";

    fn fence() -> Polygon {
        Polygon::from_pairs(&[[15.0, 15.0], [785.0, 15.0], [785.0, 385.0], [15.0, 385.0]])
    }

    fn asset(gov: &Governor) -> SimAsset {
        SimAsset::new(
            "BEE-01",
            AssetKind::Air,
            Point::new(430.0, 120.0),
            40.0,
            0.5,
            vec![
                EscalationRung::Observe,
                EscalationRung::Illuminate,
                EscalationRung::Announce,
                EscalationRung::Shadow,
                EscalationRung::Handoff,
            ],
            fence(),
            gov.public_key(),
        )
    }

    fn observe_grant(gov: &mut Governor) -> ActionGrant {
        let req = AuthorizeRequest {
            mission_id: "msn-1".into(),
            rung: EscalationRung::Observe,
            autonomy_requested: AutonomyLevel::HumanSupervised,
            asset_id: "BEE-01".into(),
            asset_state: AssetState::Docked,
            zone_id: "grounds_north".into(),
            trajectory_envelope: Polygon::from_pairs(&[
                [420.0, 110.0],
                [450.0, 110.0],
                [450.0, 250.0],
                [420.0, 250.0],
            ]),
            geofence: fence(),
            entity_confidence: 0.99,
            posture: Posture::Away,
            at: parse_ts("2026-03-14T03:11:45.5Z").unwrap(),
            operator_credential_id: None,
            operator_signature: None,
            policy_version: "compiled-default".into(),
            rule_id: "test-rule".into(),
        };
        match gov.authorize(&req).outcome {
            Outcome::Allow(g) => g,
            other => panic!("expected ALLOW, got {other:?}"),
        }
    }

    #[test]
    fn no_grant_no_motion() {
        // A grant signed by the WRONG key must be refused onboard: a
        // compromised network/adapter cannot move an asset (spec §12.3).
        let gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
        let mut fake_gov = Governor::new(KeyPair::from_seed_hex(FORGER_SEED).unwrap());
        let mut a = asset(&gov); // asset holds the REAL governor's key
        let forged = observe_grant(&mut fake_gov);
        let now = parse_ts("2026-03-14T03:11:46Z").unwrap();
        let err = a.task_observe(&forged, Point::new(440.0, 240.0), now).unwrap_err();
        assert_eq!(err, TaskRejection::Grant(GrantRejection::BadGovernorSignature));
        assert_eq!(a.state, AssetState::Docked, "and it did not move");
    }

    #[test]
    fn valid_grant_flies_to_station() {
        let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
        let mut a = asset(&gov);
        let g = observe_grant(&mut gov);
        let mut now = parse_ts("2026-03-14T03:11:53.5Z").unwrap();
        a.task_observe(&g, Point::new(440.0, 240.0), now).unwrap();
        assert_eq!(a.state, AssetState::Launching);
        for _ in 0..60 {
            a.step(0.1, now);
            now += chrono::Duration::milliseconds(100);
        }
        assert_eq!(a.state, AssetState::OnStation);
        assert_eq!(a.geofence_breaches, 0);
    }

    #[test]
    fn fleeing_target_holds_at_the_line() {
        let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
        let mut a = asset(&gov);
        let g = observe_grant(&mut gov);
        let mut now = parse_ts("2026-03-14T03:11:53.5Z").unwrap();
        a.task_observe(&g, Point::new(700.0, 120.0), now).unwrap();
        for _ in 0..120 {
            a.step(0.1, now);
            now += chrono::Duration::milliseconds(100);
        }
        // Target flees beyond the fence: asset must hold inside, masked.
        a.track_target(Point::new(845.0, 102.0));
        for _ in 0..200 {
            a.step(0.1, now);
            now += chrono::Duration::milliseconds(100);
        }
        assert!(a.geofence_hold);
        assert!(a.optics_masked);
        assert!(a.onboard_geofence.contains(&a.position), "held inside, forever");
        assert_eq!(a.geofence_breaches, 0);
    }

    #[test]
    fn watchdog_forces_return_on_link_loss() {
        let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
        let mut a = asset(&gov);
        let g = observe_grant(&mut gov);
        let mut now = parse_ts("2026-03-14T03:11:53.5Z").unwrap();
        a.task_observe(&g, Point::new(440.0, 240.0), now).unwrap();
        for _ in 0..40 {
            a.step(0.1, now);
            now += chrono::Duration::milliseconds(100);
        }
        a.set_link_lost(Some(now));
        now += chrono::Duration::seconds(6); // > 5 s limit
        a.step(0.1, now);
        assert_eq!(a.state, AssetState::Returning);
    }

    #[test]
    fn announce_requires_the_right_rung() {
        let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
        let mut a = asset(&gov);
        let g = observe_grant(&mut gov); // an OBSERVE grant
        let now = parse_ts("2026-03-14T03:11:46Z").unwrap();
        // Cannot announce on an OBSERVE grant: the verb is bound to the rung.
        assert!(a.task_announce(&g, now).is_err());
        assert_eq!(a.announcements_made, 0);
    }

    #[test]
    fn recall_needs_no_grant() {
        let mut gov = Governor::new(KeyPair::from_seed_hex(GOVERNOR_SEED).unwrap());
        let mut a = asset(&gov);
        let g = observe_grant(&mut gov);
        let now = parse_ts("2026-03-14T03:11:46Z").unwrap();
        a.task_observe(&g, Point::new(440.0, 240.0), now).unwrap();
        a.recall(); // no arguments, no grant, no key. Always available.
        assert_eq!(a.state, AssetState::Returning);
    }
}
