# drivers/matter

**Protocol:** Matter 1.x over Thread/Wi-Fi
**Capabilities exposed (§10.1 subset):** CONTACT_SENSOR, MOTION_SENSOR, ILLUMINATOR, ACCESS_POINT
**Authority tier (§3.4):** Green — OWNED
**Milestone:** M1

Locks and lighting join the DENY / ILLUMINATE rungs as fixed responders in M4 — always via a Governor grant, never via Command().

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
