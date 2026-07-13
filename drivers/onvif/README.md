# drivers/onvif

**Protocol:** ONVIF Profile S/T (RTSP media, PTZ, events)
**Capabilities exposed (§10.1 subset):** VIDEO_SOURCE, AUDIO_SOURCE, MOTION_SENSOR
**Authority tier (§3.4):** Green — OWNED (the owner's own cameras)
**Milestone:** M1

First driver to land: it is how existing estates onboard with the gear they already have. Fuzz targets (§9.1 acceptance): malformed ONVIF XML, replayed frames, 5 s clock skew — all rejected, logged, non-fatal.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
