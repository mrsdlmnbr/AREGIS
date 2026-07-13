# drivers/feed-mesh

**Protocol:** AEGIS neighbour-mesh federation (signed peer exchange)
**Capabilities exposed (§10.1 subset):** EXTERNAL_FEED
**Authority tier (§3.4):** Green — MESH_CONSENTED; connector refuses to start without a consent-record authority_ref, and revocation propagates within 60 s (§9.10)
**Milestone:** M8

Opt-in neighbour sightings corroborate entities (the mesh_corroborations receipt term). Content is UNTRUSTED input (§16.5): it can raise a number in a receipt, it can never reach a component that acts.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
