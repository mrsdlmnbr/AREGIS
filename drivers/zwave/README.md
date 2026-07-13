# drivers/zwave

**Protocol:** Z-Wave (via serial controller)
**Capabilities exposed (§10.1 subset):** CONTACT_SENSOR, MOTION_SENSOR, GLASSBREAK
**Authority tier (§3.4):** Green — OWNED
**Milestone:** M1

Legacy sensor estates. Battery + RSSI telemetry feed device_telemetry for the clock/health dashboards.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
