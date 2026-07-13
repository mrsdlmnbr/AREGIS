# ADR-0001: Record architecture decisions

Date: 2026-07-13
Status: accepted

## Context

Spec §5 mandates an append-only `docs/adr/` directory. CLAUDE.md requires an
ADR for any decision someone might later wonder about.

## Decision

We record architecture decisions as numbered markdown files in this directory,
never edited after acceptance — superseding decisions get new numbers. Format:
Context / Decision / Consequences.

## Consequences

The reasoning behind every load-bearing deviation or interpretation of the spec
is auditable in-tree, next to the code it governs.
