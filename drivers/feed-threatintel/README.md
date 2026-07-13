# drivers/feed-threatintel

**Protocol:** Licensed threat-intel / doxxing monitoring
**Capabilities exposed (§10.1 subset):** EXTERNAL_FEED
**Authority tier (§3.4):** Yellow — LICENSED, owner-authorised, revocable, logged (§3.4)
**Milestone:** M8

Digital threat chatter is an event in the same ontology (§15.4). UNTRUSTED input; prompt-injection payloads in this feed are the agent red-team suite's test corpus.

Out-of-process per §10.2: this driver runs as its own binary speaking local
gRPC to `gateway`; a crash here is a supervised restart and a `FAULT`
device, never a gateway outage. No `authority_ref`, no ingestion.
