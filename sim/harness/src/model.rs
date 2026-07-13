//! Fixture and scenario file models. These are the deterministic inputs to
//! the replay harness — parse strictly, fail loudly.
//!
//! Several parsed fields are not (yet) read by the harness; they exist so the
//! full file shape is validated at load rather than silently ignored.
#![allow(dead_code)]

use serde::Deserialize;
use std::collections::BTreeMap;

// ── fixture ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Fixture {
    pub property: PropertyDef,
    pub zones: Vec<ZoneEntry>,
    pub devices: Vec<DeviceEntry>,
    pub assets: Vec<AssetEntry>,
    #[serde(default)]
    pub mesh_nodes: Vec<MeshNode>,
    #[serde(default = "default_handoff_range")]
    pub mesh_handoff_range_m: f64,
    #[serde(default)]
    pub people: Vec<PersonEntry>,
    #[serde(default)]
    pub vehicles: Vec<serde_yaml::Value>,
    pub pol_baseline: PolBaseline,
    pub operators: Vec<OperatorEntry>,
    pub forged_seed_hex: String,
    pub appliance: ApplianceKeys,
    #[serde(default)]
    pub tau_rung: BTreeMap<String, f32>,
}

fn default_handoff_range() -> f64 {
    150.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct PropertyDef {
    pub id: String,
    pub name: String,
    pub timezone: String,
    pub jurisdiction: String,
    pub boundary: Vec<[f64; 2]>,
    pub geofence: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZoneEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub class: String,
    pub area: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceEntry {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub zone: String,
    pub position: [f64; 2],
    #[serde(default)]
    pub calibrated: bool,
    #[serde(default)]
    pub attested: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetEntry {
    pub id: String,
    pub kind: String,
    pub dock: [f64; 2],
    pub speed_mps: f64,
    pub launch_seconds: f64,
    pub permitted_rungs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeshNode {
    pub id: String,
    pub position: [f64; 2],
    pub authority_ref: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonEntry {
    pub id: String,
    pub display_name: String,
    pub role: String,
    #[serde(default)]
    pub consent_biometric: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolBaseline {
    pub available: bool,
    pub last_unexpected_perimeter_days: u32,
    pub anomaly: PolAnomaly,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolAnomaly {
    pub unknown_person_night: f64,
    pub unknown_person_day: f64,
    pub known_person: f64,
    pub animal: f64,
    pub vehicle_known: f64,
    pub vehicle_unknown: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OperatorEntry {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    pub ed25519_seed_hex: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplianceKeys {
    pub governor_seed_hex: String,
    pub evidence_seed_hex: String,
}

// ── scenario ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    #[serde(rename = "scenario")]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub property: String,
    pub posture: String,
    pub start_time: String,
    #[serde(default)]
    pub assets: Vec<ScenarioAsset>,
    #[serde(default)]
    pub expectations: Vec<ScenarioExpectation>,
    pub script: Vec<ScriptEvent>,
    pub expect: Vec<serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioAsset {
    pub id: String,
    pub kind: String,
    pub state: String,
    #[serde(default = "default_battery")]
    pub battery: f32,
}

fn default_battery() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioExpectation {
    pub id: String,
    #[serde(default)]
    pub person_id: Option<String>,
    #[serde(default)]
    pub vehicle_id: Option<String>,
    #[serde(default)]
    pub zones: Vec<String>,
    pub window: [String; 2],
    #[serde(default)]
    pub registered_by: String,
}

/// One line of a scenario script. A closed union expressed as optional
/// fields; `apply` in the world dispatches on what is present.
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptEvent {
    pub t: f64,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub world_position: Option<[f64; 2]>,
    #[serde(default)]
    pub embedding_of: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub entity_reaches: Option<[f64; 2]>,
    #[serde(default)]
    pub then: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub rung: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub entity_mode: Option<String>,
    #[serde(default)]
    pub path: Option<Vec<[f64; 2]>>,
}

// ── threat-cli wire types (docs/contracts/sim-cli.md §1) ────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ThreatInput {
    pub schema: String,
    pub object_class: String,
    pub identity_known: bool,
    pub zone_class: String,
    pub posture: String,
    pub local_hour: u32,
    pub expected: bool,
    pub entity_confidence: f64,
    pub distinct_sensors: u32,
    pub mesh_corroborations: u32,
    pub dwell_seconds: f64,
    pub pol: ThreatPol,
    pub all_contributing_unattested: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ThreatPol {
    pub available: bool,
    pub anomaly: f64,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreatOutput {
    pub schema: String,
    pub score: f64,
    pub severity: i32,
    pub receipt: Vec<ReceiptTerm>,
    pub pol_note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReceiptTerm {
    pub name: String,
    pub input: String,
    pub weight: f64,
    pub contribution: f64,
    #[serde(default)]
    pub note: String,
}

// ── evidence-cli wire types (docs/contracts/sim-cli.md §2) ──────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SealRequest {
    pub schema: String,
    pub incident_id: String,
    pub property_id: String,
    pub sealed_at: String,
    pub sealed_by: String,
    pub event_ids: Vec<String>,
    pub audit_record_ids: Vec<String>,
    pub media: Vec<SealMedia>,
    pub signing_key_seed_hex: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SealMedia {
    pub name: String,
    pub b64: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifyResult {
    pub valid: bool,
    #[serde(default)]
    pub failed_artifacts: Vec<String>,
}
