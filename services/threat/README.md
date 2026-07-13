# `threat` — deterministic severity scoring (spec §9.6)

Turns fused signals into a severity 1–5 **with a receipt**.

## The hard rule

**No neural network sets severity.** Models upstream (perception, pol)
propose inputs; this service is deterministic, compiled-in arithmetic over
them. The number that decides whether a family is woken at 03:00 must be
defensible line by line — to the operator, to the principal, to an insurer,
to a court. The base table, posture factors, term weights and severity
buckets are constants in `scoring/scoring.go`, normative per
`docs/contracts/sim-cli.md` §1. Changing any of them changes the golden
vectors here **and** in the Rust sim harness in the same commit, with an ADR.

Time is an input (axiom A7): `local_hour` arrives in the request. Nothing in
this package reads a clock. Same input, same output, forever — that is what
makes replay of an incident possible.

## Layout

- `scoring/` — the pure scorer: `Score(Input) (Output, error)`. Unknown enum
  strings are errors, never defaults (fail closed).
- `cmd/threat-cli/` — M0 sim entrypoint: `threat-cli score < in.json`.
  JSON in on stdin, one-line JSON out on stdout, errors on stderr, exit 1 on
  failure. In M1+ the same logic is served over gRPC; the CLI remains the
  deterministic test entrypoint.

## The receipt

The receipt is the feature. Every term is emitted in a fixed order with its
`input`, `weight` and `contribution` — including zero contributions, so the
operator sees that a discount did *not* apply and why. The console renders
it as-is:

```
base                  PERSON/unknown @ GROUNDS                        +2.4
posture_amplifier     AWAY @ 03h (night)                    ×2.0      +2.4
expectation_discount  no expectation matched                ×0.0      +0.0
pol_anomaly           last unexpected perimeter entity: …   ×1.5      +1.2
corroboration         3 sensors, 0 mesh                               +0.6
dwell                 0.0 s                                 ×0.5      +0.0
known_benign          none                                            −0.0
                                                            score      6.6
                                                            severity     4
```

Two receipt behaviours are load-bearing:

- `pol` unavailable → the `pol_anomaly` term contributes **0** and says
  `pol unavailable — contributing 0`. Never silently (spec §9.5).
- All contributing events unattested → severity is capped at 3 and an
  `attestation_cap` term is appended (spec §9.1).

## Verify

```
go build ./services/threat/... && go test ./services/threat/...
```

`TestS001Golden` / `TestScoreS001GoldenBytes` pin the S-001 golden vector
shared with the sim harness.
