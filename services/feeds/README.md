# `feeds` — external federation with authority enforcement (spec §9.10)

Every ingested signal carries an `Authority` (spec §3.4). **No authority, no
ingestion.** This package makes that discipline structural:

- A `Connector` declares its `Authority` tier and an `AuthorityRef` — the
  consent record, mesh agreement, or licence it operates under.
- `Registry.Start` **refuses** any connector with an empty `AuthorityRef` or
  `AUTHORITY_UNSPECIFIED`. There is no override flag and none may be added.
  Accessible is not authorized.
- `Registry.RecordDerivedEvent` maintains the eventID→connectorID index so a
  revocation can name everything the feed produced.
- `Registry.Revoke(connectorID, at)` marks the authority revoked (time is an
  input — axiom A7; the withdrawal carries its own timestamp) and returns
  the sorted derived-event IDs to be marked `authority_revoked`. **Nothing
  is deleted** — the audit record of what was ingested, and under what
  authority, outlives the authority itself.
- `RevocationDeadline(revokedAt) = revokedAt + 60s` — the spec §9.10
  acceptance bound for the data leaving the twin. Compiled in, not config.

Feed content is **untrusted input** (spec §15.5). Nothing in this package
is, or may ever become, a path to any component that can act.

M0 scope: the registry, the refusals, and the revocation index. The v1
connectors themselves (mesh · HOA · weather · wildfire · ADS-B · AIS ·
dispatch · threat-intel) arrive with M8, behind `drivers/feed-*`.

## Verify

```
go build ./services/feeds/... && go test ./services/feeds/...
```
