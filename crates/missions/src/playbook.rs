//! Playbooks as code (spec §11): declarative, versioned, regression-tested.
//! This module parses and evaluates them. `playbook-lint` (Go) statically
//! rejects unsafe files before they get here; the Governor enforces the caps
//! at runtime regardless of what a playbook says. Three layers, on purpose.

use aegis_common::types::{
    AutonomyLevel, EscalationRung, NoSuchValue, ObjectClass, Posture, ZoneClass,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct Playbook {
    #[serde(rename = "playbook")]
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: String,
    pub trigger: Trigger,
    #[serde(default)]
    pub assess: Assess,
    pub roe: Roe,
    pub actions: Vec<ActionSpec>,
    #[serde(default)]
    pub notify: serde_yaml::Value,
    #[serde(default)]
    pub evidence: Evidence,
    #[serde(default)]
    pub escalate: Vec<EscalateRule>,
    #[serde(default)]
    pub tests: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trigger {
    pub all: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Assess {
    #[serde(default)]
    pub cross_check: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Roe {
    pub max_rung: String,
    #[serde(default)]
    pub geofence: String,
    #[serde(default)]
    pub masks_offsite_optics: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionSpec {
    pub rung: String,
    pub autonomy: String,
    #[serde(default)]
    pub abort_window_seconds: Option<f64>,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub fallback_asset: Option<String>,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub stop_at: Option<String>,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
}

impl ActionSpec {
    pub fn rung(&self) -> Result<EscalationRung, NoSuchValue> {
        self.rung.parse()
    }

    pub fn autonomy(&self) -> Result<AutonomyLevel, NoSuchValue> {
        self.autonomy.parse()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Evidence {
    #[serde(default)]
    pub seal_on_trigger: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EscalateRule {
    #[serde(rename = "if")]
    pub condition: String,
    pub then: String,
}

#[derive(Debug, Error)]
pub enum PlaybookError {
    #[error("failed to parse playbook: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("playbook '{0}' declares invalid value: {1}")]
    Invalid(String, NoSuchValue),
    #[error("condition '{0}' is not parseable")]
    BadCondition(String),
}

pub fn parse(yaml: &str) -> Result<Playbook, PlaybookError> {
    let pb: Playbook = serde_yaml::from_str(yaml)?;
    // A rung the ladder does not contain must fail HERE, at load — a
    // playbook file is untrusted input to this crate (G-09 at the YAML edge).
    for a in &pb.actions {
        a.rung().map_err(|e| PlaybookError::Invalid(pb.name.clone(), e))?;
        a.autonomy().map_err(|e| PlaybookError::Invalid(pb.name.clone(), e))?;
    }
    pb.roe
        .max_rung
        .parse::<EscalationRung>()
        .map_err(|e| PlaybookError::Invalid(pb.name.clone(), e))?;
    Ok(pb)
}

/// The facts a trigger is evaluated against — everything a playbook may
/// reference. Deliberately a closed set: playbooks cannot reach into
/// arbitrary state.
#[derive(Debug, Clone)]
pub struct EntityFacts {
    pub class: ObjectClass,
    pub identity_known: bool,
    pub zone_class: ZoneClass,
    pub confidence: f64,
    pub expected: bool,
    pub dwell_seconds: f64,
}

/// Evaluate one condition line of the mini-DSL:
///   `<path> == <value>` | `<path> >= <num>` | `<path> > <num>` |
///   `<path> in [A, B]`  | `<lhs> and <rhs>`
pub fn eval_condition(cond: &str, facts: &EntityFacts, posture: Posture) -> Result<bool, PlaybookError> {
    if let Some((l, r)) = split_top_level_and(cond) {
        return Ok(eval_condition(l, facts, posture)? && eval_condition(r, facts, posture)?);
    }
    let bad = || PlaybookError::BadCondition(cond.to_string());
    let (path, op, value) = tokenize(cond).ok_or_else(bad)?;
    match op {
        "==" => Ok(lookup_str(&path, facts, posture).ok_or_else(bad)? == value),
        ">=" => {
            let lhs = lookup_num(&path, facts).ok_or_else(bad)?;
            let rhs: f64 = value.parse().map_err(|_| bad())?;
            Ok(lhs >= rhs)
        }
        ">" => {
            let lhs = lookup_num(&path, facts).ok_or_else(bad)?;
            let rhs: f64 = value.parse().map_err(|_| bad())?;
            Ok(lhs > rhs)
        }
        "in" => {
            let list = value.trim_start_matches('[').trim_end_matches(']');
            let lhs = lookup_str(&path, facts, posture).ok_or_else(bad)?;
            Ok(list.split(',').any(|item| item.trim() == lhs))
        }
        _ => Err(bad()),
    }
}

pub fn trigger_fires(pb: &Playbook, facts: &EntityFacts, posture: Posture) -> Result<bool, PlaybookError> {
    for cond in &pb.trigger.all {
        if !eval_condition(cond, facts, posture)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn split_top_level_and(cond: &str) -> Option<(&str, &str)> {
    // No brackets nesting in the DSL beyond `in [..]`; a literal " and "
    // outside brackets splits the condition.
    let mut depth = 0usize;
    let bytes = cond.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b' ' if depth == 0 && cond[i..].starts_with(" and ") => {
                return Some((&cond[..i], &cond[i + 5..]));
            }
            _ => {}
        }
    }
    None
}

fn tokenize(cond: &str) -> Option<(String, &'static str, String)> {
    for op in ["==", ">=", ">", " in "] {
        if let Some(idx) = cond.find(op) {
            let path = cond[..idx].trim().to_string();
            let value = cond[idx + op.len()..].trim().to_string();
            let op_name = match op {
                " in " => "in",
                other => other,
            };
            // Guard: ">" must not match inside ">=".
            if op == ">" && cond[idx..].starts_with(">=") {
                continue;
            }
            return Some((path, op_name, value));
        }
    }
    None
}

fn lookup_str(path: &str, facts: &EntityFacts, posture: Posture) -> Option<String> {
    match path {
        "entity.class" => Some(facts.class.name().to_string()),
        "entity.identity.known" => Some(facts.identity_known.to_string()),
        "entity.zone.class" => Some(facts.zone_class.name().to_string()),
        "entity.expected" => Some(facts.expected.to_string()),
        "posture" => Some(posture.name().to_string()),
        _ => None,
    }
}

fn lookup_num(path: &str, facts: &EntityFacts) -> Option<f64> {
    match path {
        "entity.confidence" => Some(facts.confidence),
        "entity.dwell_seconds" => Some(facts.dwell_seconds),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> EntityFacts {
        EntityFacts {
            class: ObjectClass::Person,
            identity_known: false,
            zone_class: ZoneClass::Grounds,
            confidence: 0.99,
            expected: false,
            dwell_seconds: 0.0,
        }
    }

    #[test]
    fn parses_the_shipped_playbook() {
        let yaml = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../playbooks/perimeter_breach_night.yaml"),
        )
        .unwrap();
        let pb = parse(&yaml).unwrap();
        assert_eq!(pb.name, "perimeter_breach_night");
        assert_eq!(pb.version, 4);
        assert_eq!(pb.actions.len(), 6);
        assert!(pb.evidence.seal_on_trigger);
        assert_eq!(pb.tests.len(), 6);
    }

    #[test]
    fn trigger_dsl_matches_the_night_intruder() {
        let yaml = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../playbooks/perimeter_breach_night.yaml"),
        )
        .unwrap();
        let pb = parse(&yaml).unwrap();
        assert!(trigger_fires(&pb, &facts(), Posture::Away).unwrap());
        // Known person → no fire.
        let mut known = facts();
        known.identity_known = true;
        assert!(!trigger_fires(&pb, &known, Posture::Away).unwrap());
        // Wrong posture → no fire.
        assert!(!trigger_fires(&pb, &facts(), Posture::Nominal).unwrap());
        // Expected → no fire (the cheapest line in the file).
        let mut expected = facts();
        expected.expected = true;
        assert!(!trigger_fires(&pb, &expected, Posture::Away).unwrap());
        // Animal → no fire.
        let mut animal = facts();
        animal.class = ObjectClass::Animal;
        assert!(!trigger_fires(&pb, &animal, Posture::Away).unwrap());
    }

    #[test]
    fn compound_and_condition() {
        let f = EntityFacts { dwell_seconds: 25.0, zone_class: ZoneClass::Threshold, ..facts() };
        assert!(eval_condition(
            "entity.dwell_seconds > 20 and entity.zone.class == THRESHOLD",
            &f,
            Posture::Away
        )
        .unwrap());
        assert!(!eval_condition(
            "entity.dwell_seconds > 20 and entity.zone.class == THRESHOLD",
            &facts(),
            Posture::Away
        )
        .unwrap());
    }

    #[test]
    fn a_playbook_with_a_seventh_rung_fails_to_load() {
        let yaml = r#"
playbook: bad
version: 1
trigger: { all: ["entity.class == PERSON"] }
roe: { max_rung: HANDOFF }
actions:
  - rung: FORCE
    autonomy: HUMAN_DIRECTED
"#;
        assert!(matches!(parse(yaml), Err(PlaybookError::Invalid(_, _))));
    }
}
