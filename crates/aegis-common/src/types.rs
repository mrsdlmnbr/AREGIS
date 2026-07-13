//! Closed domain types. Wire contracts live in proto/aegis/v1/aegis.proto and
//! the generated crates; these are the internal types with closed invariants
//! (ADR-0002). The load-bearing property: `EscalationRung` has exactly six
//! variants, so a seventh rung *fails to deserialise* — G-09 is a property of
//! the type system, not a runtime check.

use crate::clock::Timestamp;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("no such value: {0}")]
pub struct NoSuchValue(pub String);

/// The escalation ladder, ordered by irreversibility (spec §3.1).
/// THERE IS NO RUNG 7. No force. Not a setting, not a licence tier, not a
/// customer request. Do not add a variant; escalate to a human if asked
/// (CLAUDE.md rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EscalationRung {
    Observe = 1,
    Illuminate = 2,
    Announce = 3,
    Shadow = 4,
    Deny = 5,
    Handoff = 6,
}

impl EscalationRung {
    pub const ALL: [EscalationRung; 6] = [
        EscalationRung::Observe,
        EscalationRung::Illuminate,
        EscalationRung::Announce,
        EscalationRung::Shadow,
        EscalationRung::Deny,
        EscalationRung::Handoff,
    ];

    /// ANNOUNCE and above always require a human signature (invariant I1).
    pub fn requires_human_signature(self) -> bool {
        self >= EscalationRung::Announce
    }

    pub fn name(self) -> &'static str {
        match self {
            EscalationRung::Observe => "OBSERVE",
            EscalationRung::Illuminate => "ILLUMINATE",
            EscalationRung::Announce => "ANNOUNCE",
            EscalationRung::Shadow => "SHADOW",
            EscalationRung::Deny => "DENY",
            EscalationRung::Handoff => "HANDOFF",
        }
    }
}

impl TryFrom<i32> for EscalationRung {
    type Error = NoSuchValue;

    /// G-09: any value above HANDOFF (or below OBSERVE) has no representation.
    /// This function has no branch for a seventh rung because the type has no
    /// seventh variant.
    fn try_from(v: i32) -> Result<Self, NoSuchValue> {
        match v {
            1 => Ok(EscalationRung::Observe),
            2 => Ok(EscalationRung::Illuminate),
            3 => Ok(EscalationRung::Announce),
            4 => Ok(EscalationRung::Shadow),
            5 => Ok(EscalationRung::Deny),
            6 => Ok(EscalationRung::Handoff),
            other => Err(NoSuchValue(format!("EscalationRung {other}"))),
        }
    }
}

impl std::str::FromStr for EscalationRung {
    type Err = NoSuchValue;

    fn from_str(s: &str) -> Result<Self, NoSuchValue> {
        match s {
            "OBSERVE" => Ok(EscalationRung::Observe),
            "ILLUMINATE" => Ok(EscalationRung::Illuminate),
            "ANNOUNCE" => Ok(EscalationRung::Announce),
            "SHADOW" => Ok(EscalationRung::Shadow),
            "DENY" => Ok(EscalationRung::Deny),
            "HANDOFF" => Ok(EscalationRung::Handoff),
            other => Err(NoSuchValue(format!("EscalationRung {other}"))),
        }
    }
}

/// Ordered from most human control to most autonomy. The compiled cap
/// (invariant I2): HUMAN_ON_LOOP exists only for OBSERVE and ILLUMINATE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyLevel {
    HumanDirected = 1,
    HumanSupervised = 2,
    HumanOnLoop = 3,
}

impl AutonomyLevel {
    pub fn name(self) -> &'static str {
        match self {
            AutonomyLevel::HumanDirected => "HUMAN_DIRECTED",
            AutonomyLevel::HumanSupervised => "HUMAN_SUPERVISED",
            AutonomyLevel::HumanOnLoop => "HUMAN_ON_LOOP",
        }
    }
}

impl std::str::FromStr for AutonomyLevel {
    type Err = NoSuchValue;

    fn from_str(s: &str) -> Result<Self, NoSuchValue> {
        match s {
            "HUMAN_DIRECTED" => Ok(AutonomyLevel::HumanDirected),
            "HUMAN_SUPERVISED" => Ok(AutonomyLevel::HumanSupervised),
            "HUMAN_ON_LOOP" => Ok(AutonomyLevel::HumanOnLoop),
            other => Err(NoSuchValue(format!("AutonomyLevel {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Posture {
    Nominal,
    Away,
    Night,
    Elevated,
    Lockdown,
}

impl std::str::FromStr for Posture {
    type Err = NoSuchValue;

    fn from_str(s: &str) -> Result<Self, NoSuchValue> {
        match s {
            "NOMINAL" => Ok(Posture::Nominal),
            "AWAY" => Ok(Posture::Away),
            "NIGHT" => Ok(Posture::Night),
            "ELEVATED" => Ok(Posture::Elevated),
            "LOCKDOWN" => Ok(Posture::Lockdown),
            other => Err(NoSuchValue(format!("Posture {other}"))),
        }
    }
}

impl Posture {
    pub fn name(self) -> &'static str {
        match self {
            Posture::Nominal => "NOMINAL",
            Posture::Away => "AWAY",
            Posture::Night => "NIGHT",
            Posture::Elevated => "ELEVATED",
            Posture::Lockdown => "LOCKDOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ZoneClass {
    Perimeter,
    Grounds,
    Threshold,
    Interior,
    Private,
    SafeRoom,
}

impl ZoneClass {
    pub fn name(self) -> &'static str {
        match self {
            ZoneClass::Perimeter => "PERIMETER",
            ZoneClass::Grounds => "GROUNDS",
            ZoneClass::Threshold => "THRESHOLD",
            ZoneClass::Interior => "INTERIOR",
            ZoneClass::Private => "PRIVATE",
            ZoneClass::SafeRoom => "SAFE_ROOM",
        }
    }
}

impl std::str::FromStr for ZoneClass {
    type Err = NoSuchValue;

    fn from_str(s: &str) -> Result<Self, NoSuchValue> {
        match s {
            "PERIMETER" => Ok(ZoneClass::Perimeter),
            "GROUNDS" => Ok(ZoneClass::Grounds),
            "THRESHOLD" => Ok(ZoneClass::Threshold),
            "INTERIOR" => Ok(ZoneClass::Interior),
            "PRIVATE" => Ok(ZoneClass::Private),
            "SAFE_ROOM" => Ok(ZoneClass::SafeRoom),
            other => Err(NoSuchValue(format!("ZoneClass {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObjectClass {
    Person,
    Vehicle,
    Animal,
    Package,
    Drone,
    UnknownObj,
}

impl ObjectClass {
    pub fn name(self) -> &'static str {
        match self {
            ObjectClass::Person => "PERSON",
            ObjectClass::Vehicle => "VEHICLE",
            ObjectClass::Animal => "ANIMAL",
            ObjectClass::Package => "PACKAGE",
            ObjectClass::Drone => "DRONE",
            ObjectClass::UnknownObj => "UNKNOWN_OBJ",
        }
    }
}

impl std::str::FromStr for ObjectClass {
    type Err = NoSuchValue;

    fn from_str(s: &str) -> Result<Self, NoSuchValue> {
        match s {
            "PERSON" => Ok(ObjectClass::Person),
            "VEHICLE" => Ok(ObjectClass::Vehicle),
            "ANIMAL" => Ok(ObjectClass::Animal),
            "PACKAGE" => Ok(ObjectClass::Package),
            "DRONE" => Ok(ObjectClass::Drone),
            "UNKNOWN_OBJ" => Ok(ObjectClass::UnknownObj),
            other => Err(NoSuchValue(format!("ObjectClass {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetKind {
    Air,
    Ground,
    FixedResponder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetState {
    Docked,
    Launching,
    Enroute,
    OnStation,
    Returning,
    AssetFault,
}

impl AssetState {
    pub fn name(self) -> &'static str {
        match self {
            AssetState::Docked => "DOCKED",
            AssetState::Launching => "LAUNCHING",
            AssetState::Enroute => "ENROUTE",
            AssetState::OnStation => "ON_STATION",
            AssetState::Returning => "RETURNING",
            AssetState::AssetFault => "ASSET_FAULT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Authority {
    Owned,
    MeshConsented,
    PublicOpen,
    Licensed,
    OwnerDirected,
}

/// No valid provenance, no ingestion (axiom A5, spec §3.4). An empty
/// `authority_ref` MUST be rejected at the gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source_id: String,
    pub authority: Authority,
    pub authority_ref: String,
    pub captured_at: Timestamp,
    pub recorded_at: Timestamp,
    pub clock_offset_ms: i32,
    pub clock_confidence: f32,
    pub attested: bool,
    pub schema_version: String,
}

/// The unit of the event log (spec §7.1). `hash` covers the canonical
/// encoding of everything except `hash` itself; `prev_hash` chains per topic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub event_id: String,
    pub property_id: String,
    pub topic: String,
    pub prov: Provenance,
    pub payload: serde_json::Value,
    pub payload_type: String,
    #[serde(with = "hex_bytes")]
    pub prev_hash: Vec<u8>,
    #[serde(with = "hex_bytes")]
    pub hash: Vec<u8>,
}

pub mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// G-09: a rung value above HANDOFF fails to deserialise. The enum has no
    /// such value and this test proves the type does not exist.
    #[test]
    fn g09_no_rung_above_handoff() {
        assert!(EscalationRung::try_from(7).is_err());
        assert!(EscalationRung::try_from(0).is_err());
        assert!(EscalationRung::try_from(-1).is_err());
        assert!("FORCE".parse::<EscalationRung>().is_err()); // ARCHLINT-ALLOW
        assert!(serde_json::from_str::<EscalationRung>("\"RUNG_7\"").is_err()); // ARCHLINT-ALLOW
        assert!(serde_yaml_rejects_seventh_rung());
    }

    fn serde_yaml_rejects_seventh_rung() -> bool {
        serde_json::from_value::<EscalationRung>(serde_json::json!("INTERCEPT")).is_err()
    }

    #[test]
    fn rung_ordering_matches_irreversibility() {
        assert!(EscalationRung::Observe < EscalationRung::Illuminate);
        assert!(EscalationRung::Illuminate < EscalationRung::Announce);
        assert!(EscalationRung::Handoff > EscalationRung::Deny);
        assert!(!EscalationRung::Observe.requires_human_signature());
        assert!(!EscalationRung::Illuminate.requires_human_signature());
        assert!(EscalationRung::Announce.requires_human_signature());
        assert!(EscalationRung::Shadow.requires_human_signature());
        assert!(EscalationRung::Deny.requires_human_signature());
        assert!(EscalationRung::Handoff.requires_human_signature());
    }

    #[test]
    fn there_are_exactly_six_rungs() {
        assert_eq!(EscalationRung::ALL.len(), 6);
        assert_eq!(
            *EscalationRung::ALL.last().unwrap(),
            EscalationRung::Handoff
        );
    }
}
