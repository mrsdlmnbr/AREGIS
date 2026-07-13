# `ontology` — the digital twin (spec §9.4)

Materialises and serves the world model. The only service that will write
Postgres (axiom A1 — enforced by archlint). The twin is a *projection* of
the event log, never a source of truth.

## M0 scope

- `twin/` — the pure fold: `Apply(state, envelope)`, `Rebuild(log)`,
  `MarshalCanonical(twin)`. Deterministic by construction: sorted repeated
  fields + `proto.MarshalOptions{Deterministic: true}`. Unknown payload
  types are a **counted no-op**, never an error — logs outlive code, and a
  fold that can error can leave the twin silently partial.
- `cmd/rebuild-check/` — the M0 stand-in for `make rebuild-from-log`
  (spec §7.2, §9.4 acceptance): generates a seeded synthetic 200-event log
  in code (timestamps from a fixed base — no wall clock, axiom A7), folds
  it twice independently, byte-compares the canonical marshals. Exit 0 on
  identical, 1 with a diff summary.
- `internal/synthlog/` — the deterministic log generator shared by
  rebuild-check and the twin tests.

Postgres materialisation, `GetTwin`/`StreamTwin`/`Query` RPCs and the
`world.delta` stream arrive with M1. The fold in `twin/` is the logic those
will serve.

## The invariant

Two independent rebuilds from the same log are **byte-identical**, forever.
If rebuild-check ever fails, the fold or the canonical marshal has gone
nondeterministic — that is a broken assurance story, not a flaky check.
Find out why (CLAUDE.md).

## Verify

```
go build ./services/ontology/... && go test ./services/ontology/...
go run ./services/ontology/cmd/rebuild-check
```
