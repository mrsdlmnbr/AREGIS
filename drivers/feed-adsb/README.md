# drivers/feed-adsb

**Protocol:** ADS-B (1090 MHz SDR or licensed aggregator)
**Capabilities exposed (§10.1 subset):** EXTERNAL_FEED
**Authority tier (§3.4):** Green — PUBLIC_OPEN (broadcast) or LICENSED (aggregator); authority_ref required either way
**Milestone:** M8

A drone or aircraft loitering over the property is a real and unaddressed threat for this clientele (§9.10). Track-over-property dwell feeds the threat scorer.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
