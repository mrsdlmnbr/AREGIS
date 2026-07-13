# ADR-0005: The event-log seam, and writes that go through the log

Date: 2026-07-13
Status: accepted

## Context

M1 needs `ontology` to serve the twin over gRPC. Axiom A1 says the log is
the truth and every store is a rebuildable projection; spec §7.1 names
Redpanda as the production log. This environment (and any laptop on day one,
spec §19) must run without brokers.

## Decision

1. `services/eventlog` defines the `Log` interface (append-only,
   offset-addressed, subscribe) with two backends: `MemLog` (tests) and
   `FileLog` (one base64-of-deterministic-proto line per envelope, fsync on
   append, refuses to serve a corrupt file — I7 posture). The Redpanda
   backend lands behind the same interface with the compose stack; nothing
   above the seam may know which backend it is on.
2. Every WRITE RPC on the ontology server appends an envelope and folds its
   own append into the in-memory twin. The twin is never mutated directly,
   so a restart is a rebuild and `rebuild-check` stays byte-identical.
3. `StreamTwin` sends the full twin as its first delta, then the raw
   envelope per subsequent append — the stream is visibly a projection of
   the log, the same way the console's watch tape is.
4. Refused writes (actor-less posture change, unconsented biometrics,
   unknown structured query) touch nothing: no envelope, no fold, no ack.

## Consequences

- The M1-dev stack (`ontologyd --log world.log`) runs with zero
  infrastructure and identical semantics to the broker-backed appliance.
- Log corruption is loud and fatal at startup, never silently skipped.
- The delta stream format is cheap now and honest forever; a smarter diff
  format later is a new payload_type, not a breaking change.
