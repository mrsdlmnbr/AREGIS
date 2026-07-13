# drivers/asset-air-generic

**Protocol:** MAVLink mission API over the asset-adapter contract
**Capabilities exposed (§10.1 subset):** MOBILE_ASSET, VIDEO_SOURCE, EDGE_EMBEDDER
**Authority tier (§3.4):** Green — OWNED
**Milestone:** M7 — GATED by M3 and 1000 SITL missions + 200 live flights, zero breaches (§13)

White-label drone + dock. Mission verbs only (goto/orbit/observe/follow/return/recall); the joystick does not exist in this codebase. Onboard watchdog and geofence are the vendor-qualification bar (tools/dal-conformance).

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
