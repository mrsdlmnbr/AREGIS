# drivers/alarm-dsc

**Protocol:** DSC PowerSeries (IT-100/Envisalink bridge)
**Capabilities exposed (§10.1 subset):** CONTACT_SENSOR, MOTION_SENSOR, GLASSBREAK, ANNUNCIATOR
**Authority tier (§3.4):** Green — OWNED
**Milestone:** M1

The existing alarm panel is the L4 fallback (§16): if the appliance dies, the panel still works. ANNUNCIATOR (siren) is a rung-3 effect and fires only on a two-key grant.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
