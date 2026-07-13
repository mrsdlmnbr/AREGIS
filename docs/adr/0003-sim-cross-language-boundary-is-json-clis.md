# ADR-0003: The sim's cross-language boundary is deterministic JSON CLIs

Date: 2026-07-13
Status: accepted

## Context

M0 acceptance is a synthetic estate running end to end with zero hardware. The
scenario suite must exercise the *real* threat scorer (Go) and the *real*
evidence sealer (Go) from the Rust sim harness, which links the Rust spine
(gateway, resolver, missions, governor, asset-adapter) natively. Full gRPC +
Redpanda wiring is M1 scope (spec §21) and would add nothing to the
determinism story that M0 is required to prove.

## Decision

The sim harness drives Go components through JSON-over-stdin/stdout CLIs
(`threat-cli`, `evidence-cli`, `evidence-verify`) whose contracts are pinned in
`docs/contracts/sim-cli.md`, including a shared golden vector for scenario
S-001. Every CLI takes its timestamps as input (A7) and is bit-deterministic.

## Consequences

- `make sim` / `make replay` are deterministic across machines and time.
- The scoring arithmetic exists in exactly one implementation (Go); the Rust
  side asserts its output, so drift between languages is a replay failure, not
  a silent skew.
- In M1 the same Go packages get gRPC servers; the CLIs remain as the
  deterministic test entrypoints.
