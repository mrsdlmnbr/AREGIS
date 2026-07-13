# deploy/k3s — the appliance runtime (spec §19)

The appliance runs k3s with everything containerised, **A/B partitions with
signed OTA and automatic rollback** on health-check failure, TPM-sealed
secrets, and the Governor's signing key resident in the HSM — never in an
image, never in an env var, never in a Kubernetes Secret.

M0 ships the skeleton (`namespace.yaml`, `governor-deployment.yaml`,
`networkpolicy-camera-vlan.yaml`); the full appliance chart lands with M1
hardware bring-up. What is already binding:

- **Zero trust internally (§15.1).** The camera VLAN NetworkPolicy is deny-by
  -default: a camera can reach the gateway's ingest port and nothing else. A
  camera cannot task a drone — as a matter of routing, before it is a matter
  of cryptography.
- **The Governor pod** has no egress except its gRPC listener; its signing
  operations go to the HSM device plugin. One replica; it fails closed, so
  availability engineering must never bypass it (L3 in the degradation
  ladder is the answer to "the Governor is down", not a second authority).
- **OTA A/B** (see `../ota/README.md`): rollback is automatic, signed, and an
  audited event.
