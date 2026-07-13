# drivers/ — the Device Abstraction Layer hosts (spec §10)

Each driver is a **separate process** speaking local gRPC to `gateway`
(§10.2): a panicking driver is supervised and restarted; it cannot take the
gateway down. Drivers expose **capabilities, not model numbers** (§10.1),
and every event they emit is provenance-stamped at source — no
`authority_ref`, no ingestion (§3.4).

The contract (proto `GatewayService` + §10.2):

```
Discover()            → stream DeviceDescriptor    # capabilities, not models
Subscribe(device_id)  → stream Envelope            # provenance-stamped at source
Command(DeviceCommand)→ CommandAck                 # NEVER for assets — §13
Health()              → stream DeviceHealth
Calibrate(...)        → CalibrationResult          # homography for cameras
```

**`Command` may not move an asset.** Asset motion goes exclusively through
the Governor → asset-adapter grant path; `scripts/archlint.sh` fails the
build if any driver references asset command types.

§10.3, summarised — the five things third-party gear will not give you, and
our hardware must (this is the M9 requirements doc; `tools/dal-conformance`
is its executable form):

1. **Signed frames at capture** — chain of custody starts at the sensor.
2. **Hard time sync (PTP)** — fusion quality is bounded by clock quality.
3. **Embeddings at the edge** — vectors out, not just pixels.
4. **Survivability** — the wire will be cut; buffer locally and reconcile.
5. **Mission API, never a joystick** — and the geofence enforced onboard.

`sim-camera/` is the reference driver skeleton: deterministic, fixture-driven,
zero hardware. The per-protocol directories hold scope notes until their M1+
implementations land.
