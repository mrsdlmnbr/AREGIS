# drivers/feed-wildfire

**Protocol:** NWS / InciWeb / county emergency alert feeds
**Capabilities exposed (§10.1 subset):** EXTERNAL_FEED
**Authority tier (§3.4):** Green — PUBLIC_OPEN; authority_ref names the published feed terms
**Milestone:** M8

Wildfire and emergency alerts can raise Posture to ELEVATED automatically — posture, not action: no rung fires from a feed.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
