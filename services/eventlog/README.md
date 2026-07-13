# services/eventlog — the seam under axiom A1

The log is the truth; everything else is a rebuildable projection. This
package is the seam that makes that testable on a laptop and true on the
appliance:

| Backend | Where | Status |
|---|---|---|
| `MemLog` | tests | done |
| `FileLog` | M1-dev (`ontologyd --log world.log`), L4 local buffers | done |
| Redpanda (spec §7.1) | the appliance, via `deploy/compose` | behind this interface, lands with the broker-backed integration stack — deliberately not written until it can be integration-tested against a real broker |

Rules the interface enforces by shape:

- **Append-only.** There is no update and no delete. Retention (spec §7.1)
  is a broker/storage concern below the seam, never an API verb above it.
- **Offset-addressed reads** so any consumer can re-fold from zero — that is
  the entire recovery story (`make rebuild-from-log`, spec §7.2) and the
  black-start drill (§16).
- **Subscribers never block the log.** A slow reader is dropped, not waited
  for; the log outranks every consumer (backpressure doctrine, spec §9.1).
- **A corrupt `FileLog` refuses to serve.** Refusing loudly at startup is
  the storage-layer version of invariant I7 — silently skipping lines would
  serve a world that never happened.

Encoding note: `FileLog` stores one base64 line per envelope using
deterministic proto marshaling, because rebuild checks byte-compare
projections and a non-canonical encoding would make equal logs look
different.
