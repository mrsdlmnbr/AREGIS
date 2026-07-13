# `sync` — cross-property federation, backup, OTA (spec §9.12)

Three jobs, one axiom.

## The axiom first (A4)

**The house works with the wire cut.** Sync is *never* a dependency for
local function. Perception, fusion, playbooks, response and recording run
entirely on-premise with the WAN down. Losing this service degrades the
system to exactly L1 of the reliability ladder (spec §16): **full local
function**, no cross-property, no OTA — operator informed, not alarmed.

If a change ever makes a local code path block on sync, that change is a
bug regardless of what it fixes.

## The three jobs

| Job | What it will do (M8+) |
|---|---|
| `FederateUnknownHandlesJob` | Propagate durable `unknown:XXXX` handles between an owner's properties (spec §6.1), under explicit owner authority (`AuthorityRef` — empty refuses, spec §3.4). Embeddings travel encrypted, never in the clear (spec §3.5). |
| `EncryptedBackupJob` | Replicate the event log offsite, encrypted on the appliance before any byte leaves; keys stay in the TPM (spec §7.2). `decision.audit` is always included. |
| `SignedOTAJob` | Signed image → inactive A/B slot → verified boot → automatic rollback on failure. Unverified signature refuses before a byte is written. |

## M0 status

Typed skeletons only. Every `Run()` returns `ErrNotImplementedInM0`. The
shapes exist so the milestone plan (spec §21) is visible in the tree and so
no one designs a local dependency on a service that is allowed to be absent.

Time is an input (axiom A7): every job carries its timestamps; nothing here
reads a clock.

## Verify

```
go build ./services/sync/... && go test ./services/sync/...
```
