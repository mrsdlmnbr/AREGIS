# drivers/asset-ground-generic

**Protocol:** Vendor UGV mission API
**Capabilities exposed (§10.1 subset):** MOBILE_ASSET, VIDEO_SOURCE, AUDIO_SOURCE
**Authority tier (§3.4):** Green — OWNED
**Milestone:** M6 — the probable real v1 responder; do not let the drone's glamour reorder this (§21)

Indoor ground robot: no airspace regulator involved.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
