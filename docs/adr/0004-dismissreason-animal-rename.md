# ADR-0004: `DismissReason.ANIMAL` renamed to `DISMISS_ANIMAL`

Date: 2026-07-13
Status: accepted

## Context

The handed-over `aegis.proto` does not compile: `ANIMAL` is defined in both
`ObjectClass` (=3) and `DismissReason` (=3), and proto3 enum values are scoped
to the package (C++ scoping rules), so protoc rejects the file.

## Decision

Rename `DismissReason.ANIMAL` → `DismissReason.DISMISS_ANIMAL`, keeping the
value `3`. Binary protobuf encodes enum *numbers*, not names, so this is
wire-compatible; only generated identifiers change. This is the smallest
possible edit to the source-of-truth file, made because the file was
unbuildable as delivered — not a semantic change.

## Consequences

- Generated code refers to `DISMISS_ANIMAL`.
- JSON-form protobuf (which uses names) would serialise the new name; nothing
  in the system persists the old JSON name, since nothing could be generated
  from the broken file.
