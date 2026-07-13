# ADR-0002: Domain types vs generated wire types

Date: 2026-07-13
Status: accepted

## Context

Spec §8: `proto/aegis/v1/aegis.proto` is normative and hand-written DTOs are a
build failure. Generated protobuf types (prost/protoc-gen-go) are, however,
poor carriers for domain logic: every field is optional, enums are open i32s,
and — critically for the Governor — **an open enum would violate G-09**, which
requires that a rung value above `HANDOFF` *fails to deserialise* ("the type
does not exist").

## Decision

- `proto/aegis/v1/aegis.proto` remains the source of truth for every **wire**
  contract. `make gen` generates Rust (`crates/aegis-proto`, prost, generated
  at build time), Go (`gen/go`, committed), and Python (`py/aegis_proto`,
  committed).
- Domain crates/services use **closed internal types** (e.g. Rust
  `enum EscalationRung` with exactly six variants) with explicit, tested
  conversions to/from the generated wire types at service boundaries. A
  conversion from a wire value with no domain equivalent is a hard error —
  this is precisely how G-09 is enforced.
- The "no hand-written DTO" lint forbids *duplicating a wire message shape by
  hand for transport*. Internal domain types with closed invariants are not
  DTOs; they are the mechanism by which the wire layer's openness is contained.

## Consequences

- G-09 is enforced structurally: `EscalationRung::try_from(7)` has no branch to
  take and errors.
- Boundary conversions are explicit and unit-tested; drift between proto and
  domain types breaks the build at the conversion site.
