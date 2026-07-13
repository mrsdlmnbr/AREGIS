# Sim CLI contracts — cross-language boundaries inside `make sim`

Status: binding for M0. The deterministic sim harness (`sim/harness`, Rust) drives
the Go services through these JSON CLIs so that the scenario suite exercises the
*real* scorer and the *real* evidence sealer, not mocks. In M1+ the same logic is
served over gRPC; these CLIs remain as the deterministic test entrypoints.

Rules that bind every CLI here:
- **Time is an input (A7).** Every request carries its timestamps. No CLI may
  call the wall clock.
- **Determinism.** Same stdin → byte-identical stdout, forever.
- JSON on stdin, JSON on stdout, human-readable errors on stderr, non-zero exit
  on failure.

---

## 1. `threat-cli score` (services/threat/cmd/threat-cli)

The deterministic, explainable severity scorer of spec §9.6. **No neural network
sets severity.** Every term is emitted into the receipt with its input, weight,
and contribution.

### Input (stdin)

```json
{
  "schema": "aegis.sim.threat/v1",
  "object_class": "PERSON",
  "identity_known": false,
  "zone_class": "GROUNDS",
  "posture": "AWAY",
  "local_hour": 3,
  "expected": false,
  "entity_confidence": 0.9936,
  "distinct_sensors": 3,
  "mesh_corroborations": 0,
  "dwell_seconds": 0.0,
  "pol": {
    "available": true,
    "anomaly": 0.8,
    "note": "last unexpected perimeter entity: 41 days ago"
  },
  "all_contributing_unattested": false
}
```

- `object_class`: `PERSON|VEHICLE|ANIMAL|PACKAGE|DRONE|UNKNOWN_OBJ`
- `zone_class`: `PERIMETER|GROUNDS|THRESHOLD|INTERIOR|PRIVATE|SAFE_ROOM`
- `posture`: `NOMINAL|AWAY|NIGHT|ELEVATED|LOCKDOWN`
- `local_hour`: 0–23, property-local hour at assessment time (time is an input).
- `pol.available == false` ⇒ pol contributes **0** and the receipt says so
  (spec §9.5 failure mode). Never silently.
- `all_contributing_unattested == true` ⇒ severity is capped at 3
  (spec §9.1 attestation rule) and the receipt carries a `attestation_cap` term.

### Algorithm (normative)

```
base table (identity unknown):
              PERIMETER GROUNDS THRESHOLD INTERIOR PRIVATE SAFE_ROOM
  PERSON        2.0      2.4      3.0      4.0     4.5     5.0
  VEHICLE       1.6      2.0      2.6      3.6     4.0     4.5
  DRONE         2.2      2.6      3.0      3.6     4.0     4.5
  ANIMAL        0.2      0.2      0.4      0.8     1.0     1.0
  PACKAGE       0.4      0.4      0.8      1.0     1.2     1.2
  UNKNOWN_OBJ   1.0      1.2      1.6      2.2     2.6     3.0
identity_known == true  ⇒ base ×= 0.25

posture factor: NOMINAL 1.0 · NIGHT 1.4 · AWAY 1.6 · ELEVATED 1.8 · LOCKDOWN 2.2
night_hour = (local_hour >= 22 || local_hour < 6)
amp = factor × (night_hour ? 1.25 : 1.0)

disc = expected ? 0.9 : 0.0

pol_term  = pol.available ? pol.anomaly × 1.5 : 0.0
corr_term = min(0.3 × max(distinct_sensors − 1, 0), 0.9)
          + 0.4 × min(mesh_corroborations, 2)
dwell_term = min(dwell_seconds / 60.0, 1.0) × 0.5
benign     = (object_class == ANIMAL ? 1.0 : 0.0)
           + (identity_known && expected ? 0.5 : 0.0)

score = max(base × amp × (1 − disc) + pol_term + corr_term + dwell_term − benign, 0)

severity buckets (defaults; config per property, versioned):
  score < 1.5 → 1 · < 3.0 → 2 · < 4.5 → 3 · < 7.5 → 4 · else 5
if all_contributing_unattested: severity = min(severity, 3)
```

### Receipt (normative order and names)

Terms are emitted in this exact order, always, even when contribution is 0 —
the operator must see that a discount *didn't* apply and why:

| # | name | weight | contribution | input example |
|---|---|---|---|---|
| 1 | `base` | 1.0 | base value | `"PERSON/unknown @ GROUNDS"` |
| 2 | `posture_amplifier` | amp | `running×amp − running` | `"AWAY @ 03h (night)"` |
| 3 | `expectation_discount` | disc | `−running×disc` | `"no expectation matched"` |
| 4 | `pol_anomaly` | 1.5 | pol_term | pol.note or `"pol unavailable — contributing 0"` |
| 5 | `corroboration` | 1.0 | corr_term | `"3 sensors, 0 mesh"` |
| 6 | `dwell` | 0.5 | dwell_term | `"0.0 s"` |
| 7 | `known_benign` | 1.0 | `−benign` | `"none"` |
| 8 | `attestation_cap` | 1.0 | 0 | only present when the cap applied |

"running" is the multiplicative accumulator: after `base` it is the base value,
after `posture_amplifier` it is `base×amp`, after `expectation_discount` it is
`base×amp×(1−disc)`.

### Output (stdout)

```json
{
  "schema": "aegis.sim.threat/v1",
  "score": 6.6,
  "severity": 4,
  "receipt": [
    {"name": "base", "input": "PERSON/unknown @ GROUNDS", "weight": 1.0, "contribution": 2.4, "note": ""},
    {"name": "posture_amplifier", "input": "AWAY @ 03h (night)", "weight": 2.0, "contribution": 2.4, "note": ""},
    {"name": "expectation_discount", "input": "no expectation matched", "weight": 0.0, "contribution": 0.0, "note": ""},
    {"name": "pol_anomaly", "input": "last unexpected perimeter entity: 41 days ago", "weight": 1.5, "contribution": 1.2, "note": ""},
    {"name": "corroboration", "input": "3 sensors, 0 mesh", "weight": 1.0, "contribution": 0.6, "note": ""},
    {"name": "dwell", "input": "0.0 s", "weight": 0.5, "contribution": 0.0, "note": ""},
    {"name": "known_benign", "input": "none", "weight": 1.0, "contribution": 0.0, "note": ""}
  ],
  "pol_note": "last unexpected perimeter entity: 41 days ago"
}
```

The block above is the **golden vector for scenario S-001** (unknown person,
GROUNDS, AWAY, 03h, 3 sensors, pol anomaly 0.8): score 6.6, severity 4. It must
exist as a literal test in `services/threat`.

Numbers are formatted as JSON numbers (Go `encoding/json` default float64
formatting). `score` is rounded to 6 decimal places before encoding; term
contributions likewise.

---

## 2. `evidence-cli seal` (services/evidence/cmd/evidence-cli)

Builds an `EvidencePackage` (spec §9.9): content-addressed media, Merkle tree,
root signed. `seal_on_trigger` in a playbook routes here.

```
evidence-cli seal --out <bundle-dir>   < seal-request.json   > package.json
```

### Input

```json
{
  "schema": "aegis.sim.evidence/v1",
  "incident_id": "S-001/alert-1",
  "property_id": "ridgeline",
  "sealed_at": "2026-03-14T03:11:45.500Z",
  "sealed_by": "appliance:ridgeline",
  "event_ids": ["evt-...", "evt-..."],
  "audit_record_ids": ["aud-...", "aud-..."],
  "media": [{"name": "frame-sig-1.bin", "b64": "<base64>"}],
  "signing_key_seed_hex": "<64 hex chars>"
}
```

`signing_key_seed_hex` is an Ed25519 seed. **Sim-only affordance**: in
production the appliance HSM signs and the key never leaves it (spec §19). The
CLI must reject a request without it rather than inventing a key.

### Bundle layout (written to `--out`)

```
<bundle-dir>/
  manifest.json          # the EvidencePackage + pubkey, hex-encoded fields
  media/<sha256-hex>     # one file per artifact; the hash IS the key
```

`manifest.json`:

```json
{
  "schema": "aegis.sim.evidence/v1",
  "incident_id": "...",
  "property_id": "...",
  "sealed_at": "...",
  "sealed_by": "...",
  "event_ids": ["..."],
  "audit_record_ids": ["..."],
  "media": [{"name": "frame-sig-1.bin", "sha256": "<hex>"}],
  "merkle_root": "<hex>",
  "signature": "<hex Ed25519 over the 32 raw merkle_root bytes>",
  "signer_pubkey": "<hex>"
}
```

### Merkle construction (normative)

- Leaves, in order: `sha256(media bytes)` for each media artifact in manifest
  order, then `sha256(utf8(event_id))` for each event id, then
  `sha256(utf8(audit_record_id))` for each audit record id.
- Level up: `parent = sha256(left || right)`; an unpaired last node is promoted
  unchanged to the next level.
- Empty tree is invalid — a seal request must contain at least one leaf.

stdout is the `manifest.json` content (same bytes).

---

## 3. `evidence-verify <bundle-dir>` (tools/evidence-verify)

The standalone third-party verifier (spec §9.9). **No dependency on any other
AEGIS code path** beyond the stdlib — police, insurers, and opposing counsel run
this binary and nothing else. It must not "phone home".

- Recomputes every media artifact's SHA-256 from bytes on disk; compares to the
  manifest and to the filename.
- Rebuilds the Merkle root per the construction above.
- Verifies the Ed25519 signature over the root with `signer_pubkey`.
- Exit 0 and `{"valid": true, "failed_artifacts": []}` on stdout when the
  package verifies.
- Exit 1 and `{"valid": false, "failed_artifacts": ["<name or field>"]}` naming
  **every** failing artifact (not just the first) on any tamper.

Acceptance (spec §9.9): flip any single byte of any artifact in an exported
bundle → verify fails and names the artifact.

---

## 4. `playbook-lint <files...>` (tools/playbook-lint)

Static safety verification of playbooks (spec §11.1). Exit non-zero if ANY file
fails; report every violation, file by file.

Checks (all normative):
1. Rung names must be members of the six-rung ladder
   `OBSERVE|ILLUMINATE|ANNOUNCE|SHADOW|DENY|HANDOFF`. Anything else — including
   any seventh rung, any "force" capability — is an error with a pointed
   message citing spec §3.1. There is no rung 7.
2. Autonomy per action must not exceed the compiled cap:
   `OBSERVE, ILLUMINATE ≤ HUMAN_ON_LOOP`; `ANNOUNCE and above = HUMAN_DIRECTED`.
   A playbook may only tighten, never loosen (invariant I4).
3. `roe.max_rung` ≤ `HANDOFF` and must be a valid rung.
4. `roe.masks_offsite_optics` must be present and `true`.
5. `tests:` must list at least one scenario, and every listed scenario file
   must exist relative to the repo root.
6. `version` must be a positive integer; `playbook` name must be a
   `[a-z0-9_]+` slug.
7. `HUMAN_SUPERVISED` actions must declare `abort_window_seconds` ≥ 5.

---

## Shared receipt/severity constants

The Rust sim harness embeds the same golden S-001 vector as
`services/threat`'s unit tests. If the scoring constants change, **both**
goldens change in the same commit, with an ADR. A drifting pair is a build
failure by construction (the replay asserts the Go output).
