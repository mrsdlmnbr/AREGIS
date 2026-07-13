# AEGIS — Residential Intelligence & Response Platform

Private, on-premise intelligence and graduated response for a protected family.
The product answers one question: **is the family safe right now, and if not,
what do we do?**

Read these, in order, before touching anything:

1. [`docs/AEGIS-SPEC.md`](docs/AEGIS-SPEC.md) — the build contract.
2. [`CLAUDE.md`](CLAUDE.md) — the working agreement. Binding.
3. [`proto/aegis/v1/aegis.proto`](proto/aegis/v1/aegis.proto) — the source of
   truth for every data structure.

## The safety doctrine, in one paragraph

Response is a six-rung ladder ordered by irreversibility — `OBSERVE`,
`ILLUMINATE`, `ANNOUNCE`, `SHADOW`, `DENY`, `HANDOFF`. **There is no rung 7.**
`ANNOUNCE` and above always require a human signature. Only the deterministic
Governor (`crates/governor`) authorizes physical action, via short-lived signed
`ActionGrant`s that the asset re-verifies onboard. Models and LLMs propose;
they never authorize. Everything fails closed.

## Quick start (zero hardware)

```sh
make build      # Rust workspace + Go services + CLIs
make test       # unit + property tests (Governor: I1–I9, G-01…G-10)
make lint       # fmt, clippy, vet, architecture lints, playbook-lint
make sim        # run every scenario in sim/scenarios against the real stack
make replay SCENARIO=S-001-night-perimeter
```

`make sim` runs the deterministic scenario suite end to end — gateway
normalisation, entity fusion, deterministic threat scoring with receipts,
playbook evaluation, Governor authorization, simulated assets with onboard
watchdogs, evidence sealing, and third-party verification — with zero hardware.
The Governor does not know it is a simulation.

Requirements: Rust (stable), Go ≥ 1.24, `protoc` (for `make gen` /
`aegis-proto`), Python 3.11+ with `pytest` for the Python suites.

## Layout

See spec §5. Highlights:

| Path | What |
|---|---|
| `crates/governor` | ★ The sole authority for physical action. Highest test bar in the repo. |
| `crates/aegis-common` | Clock trait, canonical encoding, hash chain, crypto, geometry |
| `crates/gateway` · `resolver` · `missions` · `asset-adapter` | The Rust spine: ingest → fuse → decide → act |
| `services/threat` | Deterministic, explainable severity scoring — the receipt is the feature |
| `services/evidence` + `tools/evidence-verify` | Chain of custody; a third party can verify with no trust in us |
| `services/ontology` | The digital twin; the only writer of Postgres |
| `tools/playbook-lint` | A playbook may only tighten safety, never loosen it |
| `sim/` | Deterministic replay harness + scenario suite |
| `playbooks/` | Versioned response doctrine, regression-tested |

## Status

Milestone **M0** (spec §21): repo, contracts, event log, Governor with
invariants I1–I9 and test vectors G-01…G-10, deterministic simulator running a
synthetic estate end to end. See `docs/adr/` for decisions made along the way.
