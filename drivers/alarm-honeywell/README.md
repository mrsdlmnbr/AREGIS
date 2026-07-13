# drivers/alarm-honeywell

**Protocol:** Honeywell Vista (AD2 bridge)
**Capabilities exposed (§10.1 subset):** CONTACT_SENSOR, MOTION_SENSOR, GLASSBREAK, ANNUNCIATOR
**Authority tier (§3.4):** Green — OWNED
**Milestone:** M1

Same doctrine as alarm-dsc; different wire format.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
