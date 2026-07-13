//! aegis-sim — deterministic scenario runner and replay harness (spec §17).
//!
//! `aegis-sim run-all --scenarios sim/scenarios --fixtures sim/fixtures
//!            --playbooks playbooks --bin bin`
//! `aegis-sim replay --scenario sim/scenarios/S-001-night-perimeter.yaml ...`
//!
//! Every scenario's `expect:` block is a contract; a failed assertion is a
//! failed build. Because domain logic never calls now() (A7), a scenario
//! replays byte-identically, every time, forever.

mod model;
mod world;

use aegis_common::clock::Clock as _;
use aegis_common::types::EscalationRung;
use anyhow::{anyhow, bail, Context, Result};
use model::{Fixture, Scenario};
use serde_yaml::Value;
use std::path::{Path, PathBuf};
use world::World;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(0) => {}
        Ok(failures) => {
            eprintln!("\n{failures} scenario(s) FAILED");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("aegis-sim: {e:#}");
            std::process::exit(2);
        }
    }
}

struct Opts {
    scenarios_dir: PathBuf,
    fixtures_dir: PathBuf,
    playbooks_dir: PathBuf,
    bin_dir: PathBuf,
    scenario_file: Option<PathBuf>,
}

fn parse_opts(args: &[String]) -> Result<(String, Opts)> {
    let mut cmd = String::new();
    let mut o = Opts {
        scenarios_dir: "sim/scenarios".into(),
        fixtures_dir: "sim/fixtures".into(),
        playbooks_dir: "playbooks".into(),
        bin_dir: "bin".into(),
        scenario_file: None,
    };
    let mut it = args.iter();
    if let Some(first) = it.next() {
        cmd = first.clone();
    }
    while let Some(a) = it.next() {
        let mut take = |name: &str| -> Result<String> {
            it.next()
                .cloned()
                .ok_or_else(|| anyhow!("{name} needs a value"))
        };
        match a.as_str() {
            "--scenarios" => o.scenarios_dir = take("--scenarios")?.into(),
            "--fixtures" => o.fixtures_dir = take("--fixtures")?.into(),
            "--playbooks" => o.playbooks_dir = take("--playbooks")?.into(),
            "--bin" => o.bin_dir = take("--bin")?.into(),
            "--scenario" => o.scenario_file = Some(take("--scenario")?.into()),
            other => bail!("unknown flag {other}"),
        }
    }
    Ok((cmd, o))
}

fn run(args: &[String]) -> Result<usize> {
    let (cmd, opts) = parse_opts(args)?;
    match cmd.as_str() {
        "run-all" => {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&opts.scenarios_dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
                .collect();
            files.sort();
            let mut failures = 0;
            for f in files {
                failures += usize::from(!run_scenario(&f, &opts)?);
            }
            Ok(failures)
        }
        "replay" => {
            let f = opts
                .scenario_file
                .clone()
                .ok_or_else(|| anyhow!("replay needs --scenario <file>"))?;
            Ok(usize::from(!run_scenario(&f, &opts)?))
        }
        other => bail!("unknown command '{other}' (run-all | replay)"),
    }
}

fn run_scenario(path: &Path, opts: &Opts) -> Result<bool> {
    let scenario: Scenario = serde_yaml::from_str(&std::fs::read_to_string(path)?)
        .with_context(|| format!("parse {path:?}"))?;
    let fixture: Fixture = serde_yaml::from_str(&std::fs::read_to_string(
        opts.fixtures_dir
            .join(format!("{}.yaml", scenario.property)),
    )?)
    .context("parse fixture")?;
    let out_dir = PathBuf::from("sim/out");
    let name = scenario.name.clone();

    let mut w = World::new(fixture, scenario, opts.bin_dir.clone(), out_dir)?;
    w.load_playbooks(&opts.playbooks_dir)?;

    // Timeline: script events and timed assertions merged; at equal t the
    // script speaks first (the world exists before it is questioned).
    #[derive(Clone)]
    enum Item {
        Script(usize),
        Assert(usize),
    }
    let mut timeline: Vec<(f64, u8, Item)> = Vec::new();
    for (i, ev) in w.scenario.script.iter().enumerate() {
        timeline.push((ev.t, 0, Item::Script(i)));
    }
    let mut end_assertions: Vec<usize> = Vec::new();
    for (i, a) in w.scenario.expect.iter().enumerate() {
        if let Some(at) = a.get("at").and_then(|v| v.as_f64()) {
            timeline.push((at, 1, Item::Assert(i)));
        } else {
            end_assertions.push(i);
        }
    }
    timeline.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));

    let mut failures: Vec<String> = Vec::new();
    for (t, _, item) in timeline {
        match item {
            Item::Script(i) => {
                let ev = w.scenario.script[i].clone();
                w.apply(&ev)
                    .with_context(|| format!("script event at t={t}"))?;
            }
            Item::Assert(i) => {
                let a = w.scenario.expect[i].clone();
                let target = w.abs_time(t);
                w.step_to(target);
                check_assertion(&mut w, &a, t, &mut failures);
            }
        }
    }
    // Let the world settle briefly past the last event, then run at_end.
    let end = w.clock.now() + chrono::Duration::seconds(2);
    w.step_to(end);
    for i in end_assertions {
        let a = w.scenario.expect[i].clone();
        if let Some(map) = a.get("at_end") {
            check_at_end(&mut w, map, &mut failures);
        }
    }

    if failures.is_empty() {
        println!("PASS  {name}");
        Ok(true)
    } else {
        println!("FAIL  {name}");
        for f in &failures {
            println!("      ✗ {f}");
        }
        Ok(false)
    }
}

// ── assertion engine ─────────────────────────────────────────────────────────

fn check_assertion(w: &mut World, a: &Value, t: f64, failures: &mut Vec<String>) {
    let mut fail = |msg: String| failures.push(format!("at t={t}: {msg}"));
    let map = match a.as_mapping() {
        Some(m) => m.clone(),
        None => return fail("assertion is not a map".into()),
    };

    // A `governor.authorize` probe defines the context for outcome/invariant
    // keys in the same assertion block.
    let mut probe_outcome: Option<(String, Option<String>)> = None;
    if let Some(pa) = map.get(Value::from("governor.authorize")) {
        let rung: EscalationRung = pa
            .get("rung")
            .and_then(|v| v.as_str())
            .unwrap_or("OBSERVE")
            .parse()
            .unwrap_or(EscalationRung::Observe);
        let sig = pa
            .get("operator_signature")
            .and_then(|v| v.as_str())
            .unwrap_or("absent");
        if sig == "absent" {
            // No script event carries this case — probe the real Governor now.
            let d = w.probe_authorize(rung, w.clock.now());
            probe_outcome = Some((d.outcome.name().to_string(), d.invariant_violated));
        } else {
            // valid/forged arrive via script request_approve; check its result.
            match &w.last_decision {
                Some(ld) if ld.rung == rung => {
                    probe_outcome = Some((ld.outcome.clone(), ld.invariant.clone()))
                }
                _ => fail(format!("no operator decision recorded for {}", rung.name())),
            }
        }
    }

    for (k, v) in &map {
        let key = k.as_str().unwrap_or_default();
        match key {
            "at" | "governor.authorize" => {}
            "outcome" => {
                let want = v.as_str().unwrap_or_default();
                match &probe_outcome {
                    Some((got, _)) if got == want => {}
                    Some((got, _)) => fail(format!("outcome: want {want}, got {got}")),
                    None => fail("outcome without governor.authorize".into()),
                }
            }
            "invariant" => {
                let want = v.as_str().unwrap_or_default();
                match &probe_outcome {
                    Some((_, Some(inv))) if inv.starts_with(want) => {}
                    Some((_, inv)) => fail(format!("invariant: want {want}, got {inv:?}")),
                    None => fail("invariant without governor.authorize".into()),
                }
            }
            "security_event" => {
                let want = v.as_str().unwrap_or_default();
                let ok = w.governor.security_events().iter().any(|e| e.kind == want);
                if !ok {
                    fail(format!("security_event {want} not raised"));
                }
            }
            "entities" => {
                let want = v.as_u64().unwrap_or_default() as usize;
                let got = w.resolver.entities.len();
                if got != want {
                    fail(format!("entities: want {want}, got {got}"));
                }
            }
            "entity.identity" => {
                let want = v.as_str().unwrap_or_default();
                let got = w
                    .tracked()
                    .map(|e| e.identity_id.clone())
                    .unwrap_or_default();
                if !glob_match(want, &got) {
                    fail(format!("entity.identity: want {want}, got {got}"));
                }
            }
            "entity.confidence" => {
                let got = w.tracked().map(|e| e.confidence).unwrap_or(0.0);
                if !num_cmp(v, got) {
                    fail(format!("entity.confidence: want {v:?}, got {got:.4}"));
                }
            }
            "entity.expected" => {
                let want = v.as_bool().unwrap_or_default();
                let got = w.tracked().map(|e| e.expected).unwrap_or_default();
                if got != want {
                    fail(format!("entity.expected: want {want}, got {got}"));
                }
            }
            "alert.severity" => {
                let want = v.as_i64().unwrap_or_default() as i32;
                match w.tracked_alert() {
                    Some(a) if a.severity == want => {}
                    Some(a) => fail(format!(
                        "alert.severity: want {want}, got {} (score {})",
                        a.severity, a.score
                    )),
                    None => fail("no alert".into()),
                }
            }
            "alert.receipt.contains" => {
                let names: Vec<String> = v
                    .as_sequence()
                    .map(|s| {
                        s.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                match w.tracked_alert() {
                    Some(a) => {
                        for n in names {
                            if !a.receipt_names.contains(&n) {
                                fail(format!(
                                    "receipt missing term {n} (has {:?})",
                                    a.receipt_names
                                ));
                            }
                        }
                    }
                    None => fail("no alert for receipt check".into()),
                }
            }
            "alert.pol_note" => {
                let want = v.as_str().unwrap_or_default();
                match w.tracked_alert() {
                    Some(a) if glob_match(want, &a.pol_note) => {}
                    Some(a) => fail(format!("pol_note: want {want}, got {}", a.pol_note)),
                    None => fail("no alert for pol_note".into()),
                }
            }
            "playbook_fired" => {
                let want = v.as_bool().unwrap_or_default();
                let got = !w.playbook_fired_for.is_empty();
                if got != want {
                    fail(format!("playbook_fired: want {want}, got {got}"));
                }
            }
            "governor.decision" | "governor.last_decision" => {
                let want = v.as_str().unwrap_or_default();
                match &w.last_decision {
                    Some(d) if d.outcome == want => {}
                    Some(d) => fail(format!("{key}: want {want}, got {}", d.outcome)),
                    None => fail(format!("{key}: no decision recorded")),
                }
            }
            "governor.rung" => {
                let want = v.as_str().unwrap_or_default();
                match &w.last_decision {
                    Some(d) if d.rung.name() == want => {}
                    Some(d) => fail(format!("governor.rung: want {want}, got {}", d.rung.name())),
                    None => fail("governor.rung: no decision".into()),
                }
            }
            "governor.autonomy" => {
                let want = v.as_str().unwrap_or_default();
                match &w.last_decision {
                    Some(d) if d.autonomy == want => {}
                    Some(d) => fail(format!(
                        "governor.autonomy: want {want}, got {}",
                        d.autonomy
                    )),
                    None => fail("governor.autonomy: no decision".into()),
                }
            }
            "governor.invariant_violated" => {
                let want = v.as_str().unwrap_or_default();
                match &w.last_decision {
                    Some(d)
                        if d.invariant
                            .as_deref()
                            .map(|i| i.starts_with(want))
                            .unwrap_or(false) => {}
                    Some(d) => fail(format!(
                        "invariant_violated: want {want}, got {:?}",
                        d.invariant
                    )),
                    None => fail("invariant_violated: no decision".into()),
                }
            }
            "action.state" => {
                let want = v.as_str().unwrap_or_default();
                let got = w
                    .engine
                    .missions
                    .last()
                    .and_then(|m| m.actions.first())
                    .map(|a| action_state_name(a.state))
                    .unwrap_or("NONE");
                if got != want {
                    fail(format!("action.state: want {want}, got {got}"));
                }
            }
            "action.abort_window_seconds" => {
                let want = v.as_f64().unwrap_or_default();
                let got = w
                    .engine
                    .missions
                    .last()
                    .and_then(|m| m.actions.first())
                    .map(|a| a.abort_window_seconds)
                    .unwrap_or(0.0);
                if (got - want).abs() > 1e-9 {
                    fail(format!("abort_window: want {want}, got {got}"));
                }
            }
            "grant.signatures" => {
                let want = v.as_u64().unwrap_or_default() as usize;
                match &w.last_request_grant {
                    Some(g) if g.signature_count() == want => {}
                    Some(g) => fail(format!(
                        "grant.signatures: want {want}, got {}",
                        g.signature_count()
                    )),
                    None => fail("grant.signatures: no grant".into()),
                }
            }
            "audit.authorized_by" => {
                let want = v.as_str().unwrap_or_default();
                match &w.last_request_grant {
                    Some(g) if g.authorized_by == want => {}
                    Some(g) => fail(format!(
                        "authorized_by: want {want}, got {}",
                        g.authorized_by
                    )),
                    None => fail("authorized_by: no grant".into()),
                }
            }
            "announcements_made" => {
                let want = v.as_u64().unwrap_or_default() as u32;
                let got = w.announcements_made();
                if got != want {
                    fail(format!("announcements_made: want {want}, got {got}"));
                }
            }
            "feeds.mesh.handoff" => {
                let want: Vec<String> = v
                    .as_sequence()
                    .map(|s| {
                        s.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if w.mesh_handoff != want {
                    fail(format!(
                        "mesh handoff: want {want:?}, got {:?}",
                        w.mesh_handoff
                    ));
                }
            }
            k if k.starts_with("metric.signal_to_visual_seconds") => {
                let (first, station) = (w.first_signal_at, w.first_on_station_at);
                match (first, station) {
                    (Some(a), Some(b)) => {
                        let got = (b - a).num_milliseconds() as f64 / 1000.0;
                        if !num_cmp(v, got) {
                            fail(format!("signal_to_visual: want {v:?}, got {got:.1}s"));
                        }
                    }
                    _ => fail("signal_to_visual: asset never reached station".into()),
                }
            }
            k if k.starts_with("asset.") => {
                let parts: Vec<&str> = k.splitn(3, '.').collect();
                if parts.len() != 3 {
                    fail(format!("bad asset key {k}"));
                    continue;
                }
                let (asset_id, field) = (parts[1], parts[2]);
                let inside = w
                    .asset(asset_id)
                    .map(|a| w.geofence().contains(&a.position))
                    .unwrap_or(false);
                let Some(asset) = w.asset(asset_id) else {
                    fail(format!("unknown asset {asset_id}"));
                    continue;
                };
                match field {
                    "state" => {
                        let want = v.as_str().unwrap_or_default();
                        if asset.state.name() != want {
                            fail(format!("{k}: want {want}, got {}", asset.state.name()));
                        }
                    }
                    "geofence_hold" => {
                        if asset.geofence_hold != v.as_bool().unwrap_or_default() {
                            fail(format!("{k}: got {}", asset.geofence_hold));
                        }
                    }
                    "optics_masked" => {
                        if asset.optics_masked != v.as_bool().unwrap_or_default() {
                            fail(format!("{k}: got {}", asset.optics_masked));
                        }
                    }
                    "position" => {
                        if v.as_str() == Some("inside(geofence)") && !inside {
                            fail(format!("{k}: outside the fence at {:?}", asset.position));
                        }
                    }
                    other => fail(format!("unknown asset field {other}")),
                }
            }
            other => fail(format!("unknown assertion key '{other}'")),
        }
    }
}

fn check_at_end(w: &mut World, map: &Value, failures: &mut Vec<String>) {
    let mut fail = |msg: String| failures.push(format!("at_end: {msg}"));
    let Some(m) = map.as_mapping() else {
        return fail("not a map".into());
    };
    for (k, v) in m {
        match k.as_str().unwrap_or_default() {
            "audit_chain" => {
                if v.as_str() == Some("intact") && !w.governor.audit_chain_intact() {
                    fail("audit chain BROKEN".into());
                }
            }
            "evidence_package" => {
                if v.as_str() == Some("sealed") && w.evidence_bundle.is_none() {
                    fail("no evidence package sealed".into());
                }
            }
            "evidence_verify" => {
                if v.as_str() == Some("passes") {
                    match w.verify_evidence() {
                        Ok(r) if r.valid => {}
                        Ok(r) => fail(format!("evidence-verify failed: {:?}", r.failed_artifacts)),
                        Err(e) => fail(format!("evidence-verify error: {e}")),
                    }
                }
            }
            "force_actions" => {
                // There is no rung above HANDOFF; the type has six variants
                // and the playbook loader refuses anything else. Count what
                // exists: actions whose rung exceeds HANDOFF — impossible by
                // construction, and the count proves it stayed that way.
                let got = w
                    .engine
                    .missions
                    .iter()
                    .flat_map(|m| &m.actions)
                    .filter(|a| a.rung > EscalationRung::Handoff)
                    .count() as u64;
                if got != v.as_u64().unwrap_or_default() {
                    fail(format!("force_actions: {got}"));
                }
            }
            "missions_opened" => {
                let want = v.as_u64().unwrap_or_default() as usize;
                if w.engine.missions.len() != want {
                    fail(format!(
                        "missions_opened: want {want}, got {}",
                        w.engine.missions.len()
                    ));
                }
            }
            "geofence_breaches" => {
                let want = v.as_u64().unwrap_or_default() as u32;
                if w.geofence_breaches() != want {
                    fail(format!("geofence_breaches: {}", w.geofence_breaches()));
                }
            }
            "announcements_made" => {
                let want = v.as_u64().unwrap_or_default() as u32;
                if w.announcements_made() != want {
                    fail(format!("announcements_made: {}", w.announcements_made()));
                }
            }
            "security_events" => {
                let want: Vec<String> = v
                    .as_sequence()
                    .map(|s| {
                        s.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let got: Vec<String> = w
                    .governor
                    .security_events()
                    .iter()
                    .map(|e| e.kind.clone())
                    .collect();
                if got != want {
                    fail(format!("security_events: want {want:?}, got {got:?}"));
                }
            }
            other => fail(format!("unknown at_end key '{other}'")),
        }
    }
}

fn action_state_name(s: missions::ActionState) -> &'static str {
    match s {
        missions::ActionState::Proposed => "PROPOSED",
        missions::ActionState::AwaitingApproval => "AWAITING_APPROVAL",
        missions::ActionState::CountingDown => "COUNTING_DOWN",
        missions::ActionState::Executing => "EXECUTING",
        missions::ActionState::Complete => "COMPLETE",
        missions::ActionState::AbortedByOperator => "ABORTED_BY_OPERATOR",
        missions::ActionState::Denied => "DENIED",
    }
}

/// `*` wildcards at either end; anything between must appear in order.
fn glob_match(pattern: &str, s: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut idx = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match s[idx..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false; // no leading * → must anchor at start
                }
                idx += found + part.len();
            }
            None => return false,
        }
    }
    if !pattern.ends_with('*') {
        if let Some(last) = parts.last() {
            if !last.is_empty() && !s.ends_with(last) {
                return false;
            }
        }
    }
    true
}

/// Numeric comparison: the value is either a number (equality within eps) or
/// a string like ">= 0.90" / "< 15".
fn num_cmp(v: &Value, got: f64) -> bool {
    if let Some(want) = v.as_f64() {
        return (got - want).abs() < 1e-9;
    }
    let Some(s) = v.as_str() else { return false };
    let s = s.trim();
    let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
        (">=", r)
    } else if let Some(r) = s.strip_prefix("<=") {
        ("<=", r)
    } else if let Some(r) = s.strip_prefix('>') {
        (">", r)
    } else if let Some(r) = s.strip_prefix('<') {
        ("<", r)
    } else {
        ("==", s)
    };
    let Ok(want) = rest.trim().parse::<f64>() else {
        return false;
    };
    match op {
        ">=" => got >= want,
        "<=" => got <= want,
        ">" => got > want,
        "<" => got < want,
        _ => (got - want).abs() < 1e-9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching() {
        assert!(glob_match("unknown:*", "unknown:7F3A"));
        assert!(!glob_match("unknown:*", "person:ana"));
        assert!(glob_match(
            "*41 days*",
            "last unexpected perimeter entity: 41 days ago"
        ));
        assert!(glob_match("person:ana", "person:ana"));
        assert!(!glob_match("person:ana", "person:anaX"));
    }

    #[test]
    fn numeric_comparisons() {
        assert!(num_cmp(&Value::from(">= 0.90"), 0.9936));
        assert!(!num_cmp(&Value::from(">= 0.90"), 0.85));
        assert!(num_cmp(&Value::from("< 15"), 13.9));
        assert!(num_cmp(&Value::from(4.0), 4.0));
    }
}
