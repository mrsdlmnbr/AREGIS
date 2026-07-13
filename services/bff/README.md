# `bff` — backend-for-frontend (spec §9.13)

gRPC-web + WebSocket for the console and apps, WebRTC signalling for video.
Enforces RBAC per persona.

## The doctrine

**The bff has no authority of its own.** It is a projection and a relay.

- It holds no Governor credential and no asset credential.
- Its route table cannot name `GovernorService` or `AssetService` at all —
  those namespaces are compiled into a forbidden list (`Validate`), and the
  test `TestNoPersonaRoutesToGovernorOrAssetTask` pins it. Physical
  authority flows exclusively missions → governor → asset-adapter → asset
  (spec §12, §13).
- An operator approving an action goes through
  `MissionService/ApproveAction`; the approval carries the operator's
  signature, and it is the Governor — behind missions — that authorizes.
- Deny-by-default: a method not listed for a persona does not exist for
  that persona (fail closed, CLAUDE.md rule 5).

A compromised bff therefore yields a read of the world and some UI verbs —
never a moved machine.

## Personas (spec §1.2, §14)

| Persona | Surface | Gets |
|---|---|---|
| `OPERATOR` | console (§14.1) | full read, alert handling, mission verbs (relay only) |
| `PRINCIPAL` | owner app (§14.2) | status, posture, their data: export/revoke, audit visibility |
| `STAFF` | staff app (§14.3) | expectations, people, access grants — the false-alarm machine |
| `PARTNER` | detail / LE liaison | sealed evidence export + verify, nothing else |

## M0 scope

Persona enum + route table + validation. Transport (gRPC-web, WS, WebRTC
signalling) arrives with M1's console.

## Verify

```
go build ./services/bff/... && go test ./services/bff/...
```
