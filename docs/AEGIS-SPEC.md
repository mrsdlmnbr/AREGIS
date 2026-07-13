# AEGIS — Master Build Specification
### Residential Intelligence & Response Platform
**Version 1.0 · Engineering handover · Buildable by an AI agent under human review**

---

## 0. How to use this document

This is not a vision memo. It is a build contract.

**If you are the implementing agent:**

1. Read this document end to end before writing code. Then read `CLAUDE.md` — it is the working agreement and it binds you.
2. `proto/aegis/v1/aegis.proto` is the **source of truth** for every data structure. Never hand-write a DTO. Generate.
3. Build in the milestone order in §22. Do not skip ahead. Milestone 3 has a **kill gate** — if it fails, the drone work in Milestone 7 must never begin.
4. §3 (Safety Doctrine) and §12 (Governor) contain constraints that are **not negotiable and not configurable**. If a requirement elsewhere in this document appears to conflict with them, §3 and §12 win, and you must stop and raise it.
5. Anything you cannot implement safely, you do not implement. Say so.

**If you are the human reviewing:** §3, §12, §13, §16 and §21 are the sections where a mistake kills someone or kills the company. Review them like flight software. Everything else is recoverable.

---

## 1. Product definition

### 1.1 What this is

A private intelligence and response platform for a protected family. It ingests every lawful signal around their properties, fuses it into a single world model, decides what matters, and drives a graduated physical response — all on hardware the family owns, with a professional operator in the loop.

**The question the product answers, and the only one:**
> *Is the family safe right now, and if not, what do we do?*

### 1.2 Personas

| Persona | Surface | What they need |
|---|---|---|
| **Operator** — professional, 8-hour watch, often 03:00 | Console | Dense, fast, keyboard-first. Signal → decision in seconds. |
| **Principal** — the family | Owner app | Silence they can trust. Interrupted only for decisions only they can make. |
| **Estate staff** — manager, housekeeper | Staff app | Their zones, their schedule, and the ability to *pre-register expected visitors*. |
| **Detail / monitoring centre / insurer** | Partner API | Verified events, sealed evidence, live handoff. |

### 1.3 Non-goals — explicitly out of scope, permanently

- Any capability that applies force to a person. No payloads, no interception, no vehicle disablement, no "non-lethal" anything.
- Ingestion of any feed we do not hold a demonstrable right to.
- Facial recognition against public or third-party databases. Biometrics are **enrolment-only, consented, and local**.
- Consumer mass-market SKU (not in v1; the architecture permits it later, the go-to-market does not).

---

## 2. Design axioms

Load-bearing. An implementation that violates one is wrong even if it passes its tests.

| # | Axiom | Consequence for the code |
|---|---|---|
| **A1** | **The log is the truth.** An append-only, signed, provenance-stamped event log is the sole source of truth. Ontology, alerts, timeline, and every UI are *projections*. | No service writes state except by appending an event. `ontology` is a materialised view. |
| **A2** | **Stochastic proposes, deterministic authorizes.** Models and LLMs may *propose*. Only the Governor may *authorize* a physical action. | No neural component sits in any authorization path. Enforced by an architecture test. |
| **A3** | **Autonomy is capped by consequence.** Rungs are ordered by irreversibility. **`ANNOUNCE` and above always require a human.** | Compiled in. Policy files may only *tighten* caps, never loosen them. |
| **A4** | **The house works with the wire cut.** | Full function — perception, fusion, playbooks, response, recording — on-premise with the WAN down. |
| **A5** | **Provenance on every byte.** Every event knows its source, its capture clock, and the **authority** under which it was collected. | An event without valid `Provenance` is rejected at the gateway. No exceptions. |
| **A6** | **Devices are commodity; the abstraction is the asset.** | Third-party gear implements a subset of the capability interfaces. Our hardware implements the superset. One codebase, forever. |
| **A7** | **Time is an input, never an ambient.** Domain logic never calls `now()`. | This is what makes deterministic replay possible, which is what makes autonomous physical response defensible. |

---

## 3. Safety & legal doctrine — binding constraints

These are requirements, not guidance. They are restated in the Governor spec (§12) and enforced in code.

### 3.1 The escalation ladder

Response is graduated, ordered by irreversibility. **95% of the value is in rungs 1–2.**

| # | Rung | Meaning | Autonomy permitted |
|---|---|---|---|
| 1 | `OBSERVE` | Launch, look, stream. No interaction with any person. | up to `HUMAN_ON_LOOP` |
| 2 | `ILLUMINATE` | Grounds lighting, asset spotlight. | up to `HUMAN_ON_LOOP` |
| 3 | `ANNOUNCE` | Voice warning, siren. | **`HUMAN_DIRECTED` only** |
| 4 | `SHADOW` | Follow and record. Stops at the geofence. | **`HUMAN_DIRECTED` only** |
| 5 | `DENY` | Lock interior zones, close gate, secure safe room. Passive. | **`HUMAN_DIRECTED` only** |
| 6 | `HANDOFF` | Sealed evidence + live feed to detail and law enforcement. | **`HUMAN_DIRECTED` only** |

**There is no rung 7.** No force. Not a setting, not a licence tier, not a customer request. The enum has no seventh value and the Governor has no branch for one.

### 3.2 Geofence — triple enforcement

The asset must be **incapable** of the thing the Governor would forbid.

1. **Governor** — denies authorization for any action whose trajectory envelope leaves `roe.geofence`.
2. **Mission executor** — clips every waypoint; refuses to issue a command outside the fence.
3. **Asset firmware** — holds the geofence onboard, verifies the `geofence_hash` in the grant against its loaded fence, and hard-stops at the line. **An independent onboard watchdog forces return-to-dock on breach.**

A compromised console, a compromised network, and a compromised mission service must all, independently, be unable to fly an asset over the neighbour's pool.

### 3.3 Optics masking

When an asset's field of view crosses the property line, optics are masked in the encoder — not in the UI. The masked frames are what get recorded and what get signed. Every frame's pose is logged. This is the defence against the first lawsuit and the first news story.

### 3.4 Authority model for data

Every ingested signal carries an `Authority`. No authority, no ingestion.

| Tier | Examples | `Authority` |
|---|---|---|
| **Green** | Owner's own devices · opt-in neighbour mesh · HOA cameras · weather · wildfire/emergency · ADS-B · AIS · published dispatch feeds | `OWNED`, `MESH_CONSENTED`, `PUBLIC_OPEN` |
| **Yellow** | Law-enforcement footage-sharing programs · licensed vehicle data | `OWNER_DIRECTED`, `LICENSED` — gated, logged, owner-authorised, revocable |
| **Red — do not build** | Any camera or feed we do not hold rights to. Facial recognition against external databases. | — |

The temptation to quietly tap "just one more feed" is the thing that kills a company like this. The provenance layer exists so the discipline is **structural, not optional**.

### 3.5 Biometrics

Enrolment-only, consented, revocable, stored as embeddings (never raw imagery of a face used as a template), **never leaves the appliance** except as an encrypted cross-property sync under explicit owner authority. A person may be un-enrolled and their embeddings destroyed; the destruction is itself an audit record.

### 3.6 Regulatory dependencies (verify before Milestone 7)

Airspace rules for autonomous flight change. **Do not encode a regulatory assumption in code.** The drone tier ships behind a jurisdiction policy file, defaults to the most restrictive interpretation, and requires a credentialed remote pilot on the console. Verify the current BVLOS framework and local rules at the start of M7, not at the start of the project.

---

## 4. System context

```
┌─ EDGE ────────────────────────────────────────────────────────────────┐
│ cameras · motion · contact · glassbreak · locks · alarm panel         │
│ air asset (drone + dock) · ground asset (UGV) · fixed responders      │
│ network tap · external feeds (mesh, weather, ADS-B, threat intel)     │
└──────────────────────────────┬───────────────────────────────────────┘
                               │ DAL drivers (§10)
┌─ APPLIANCE (on-premise, per property) ───────────────────────────────┐
│                                                                       │
│  gateway ──▶ [EVENT LOG] ──▶ perception ──▶ resolver ──▶ ontology    │
│                  │                                          │         │
│                  ├────────────────────────▶ pol ───────────┤         │
│                  │                                          ▼         │
│                  │                                       threat       │
│                  │                                          │         │
│                  │                                          ▼         │
│                  │                    ┌──────────────── missions      │
│                  │                    │                     │         │
│                  │                    ▼                     ▼         │
│                  │              ★ GOVERNOR ★ ──grant──▶ asset-adapter │
│                  │                    │                     │         │
│                  ├── evidence ◀───────┘                     ▼         │
│                  ├── feeds                             [ROBOT/ASSET]  │
│                  ├── agent (read-only, no credentials)                │
│                  └── sync ──────────────────────────────▶ cloud/peers │
│                                                                       │
│                          bff ──▶ console · owner app · staff · partner│
└───────────────────────────────────────────────────────────────────────┘
```

**The single most important arrow in this diagram is `GOVERNOR ──grant──▶ asset-adapter`.** It is the only path by which anything physical moves. See §12.

---

## 5. Repository layout

```
aegis/
├── CLAUDE.md                    # working agreement — binding on the agent
├── Makefile                     # make gen | build | test | replay | sim | lint
├── docs/
│   ├── AEGIS-SPEC.md            # this document
│   └── adr/                     # architecture decision records, append-only
├── proto/aegis/v1/aegis.proto   # SOURCE OF TRUTH for all contracts
├── db/schema.sql                # Postgres DDL (materialised views over the log)
│
├── crates/                      # Rust — anything that can move a robot
│   ├── aegis-common/            # types, canonical encoding, crypto, clock trait
│   ├── gateway/                 # DAL host, normalisation, provenance stamping
│   ├── resolver/                # sighting → track → entity → identity
│   ├── missions/                # playbook engine, mission lifecycle
│   ├── governor/                # ★ sole authority for physical action
│   └── asset-adapter/           # grant verification + mission→robot protocol
│
├── services/                    # Go
│   ├── ontology/                # world model store + twin stream
│   ├── threat/                  # explainable severity scoring
│   ├── evidence/                # hash chain, merkle seal, export
│   ├── feeds/                   # external federation + authority enforcement
│   ├── sync/                    # cross-property, backup, signed OTA
│   └── bff/                     # backend-for-frontend: gRPC-web, WS, WebRTC signalling
│
├── py/                          # Python — models only, never authority
│   ├── perception/              # detection, tracking, embeddings, audio
│   ├── pol/                     # pattern-of-life baselines and anomaly
│   └── agent/                   # LLM copilot — READ-ONLY, no credentials
│
├── drivers/                     # DAL drivers, one per protocol
│   ├── onvif/ matter/ zwave/ alarm-dsc/ alarm-honeywell/
│   ├── asset-air-generic/ asset-ground-generic/
│   └── feed-mesh/ feed-adsb/ feed-wildfire/ feed-threatintel/
│
├── web/console/                 # operator console (React + TS)
├── apps/owner/  apps/staff/     # React Native
├── playbooks/                   # versioned YAML, each with a regression suite
├── sim/
│   ├── harness/                 # deterministic replay + scenario runner
│   ├── scenarios/               # YAML incident scripts
│   └── sitl/                    # software-in-the-loop for assets (PX4/Gazebo)
├── deploy/                      # compose (dev), k3s (appliance), OTA A/B
└── tools/
    ├── evidence-verify/         # third party can verify a sealed package
    ├── playbook-lint/           # static checks + safety-cap verification
    └── dal-conformance/         # ← the test suite our future hardware must pass
```

`tools/dal-conformance` is deliberately in the tree from day one. **It is the acceptance test for the cameras, drones and robots you will eventually build.**

---

## 6. Domain model

### 6.1 The resolution pipeline

Four levels. Collapse any two and the system degrades into a motion-clip app.

```
SIGHTING   one detection · one sensor · one instant
    ↓  temporal association within a single sensor
TRACK      a continuous observation on one sensor
    ↓  spatial (ground-plane) + appearance (embedding) association across sensors
ENTITY     one thing moving through the world, seen by many sensors
    ↓  gallery match against enrolled records, else durable pseudonymous handle
IDENTITY   person:ana · vehicle:8XJ-4410 · unknown:7F3A
```

**`unknown:7F3A` is the product.** A durable handle for a stranger. The unknown at the gate on Tuesday and the unknown at the Aspen property on Friday resolve to the *same unknown* — tracked across sensors, properties and weeks, without ever knowing who they are. This is why the wealthy-first wedge is correct: the value only exists at multi-property, multi-sensor complexity, and it is exactly what no consumer product can do.

### 6.2 The two objects everyone forgets

- **`Expectation`** — the housekeeper is expected Tuesday 09:00–13:00; the contractor was pre-registered by the estate manager this morning. **Expectations are the single largest false-alarm reduction in the system and they cost almost nothing to build.** Ship them in M3, before any model work.
- **`Posture`** — `NOMINAL · AWAY · NIGHT · ELEVATED · LOCKDOWN`. Modulates every rule. The same person on the terrace is a non-event at 14:00/NOMINAL and a severity 4 at 03:00/AWAY.

### 6.3 Object catalogue

`Property · Zone · Device · Asset · Dock · Person · AccessGrant · Expectation · Vehicle · Sighting · Track · Entity · Identity · Alert · Playbook · Mission · RulesOfEngagement · Action · ActionGrant · Feed · EvidencePackage · AuditRecord · Posture`

Canonical definitions: `proto/aegis/v1/aegis.proto`. Do not restate them anywhere else.

---

## 7. Data architecture

### 7.1 The event log

- **Redpanda** (Kafka API), single-node on the appliance, tiered to MinIO.
- Topics (all keyed by `property_id`, ordered per key):

| Topic | Producer | Retention |
|---|---|---|
| `raw.device` | gateway | 7 d hot, 90 d cold |
| `perception.sighting` | perception | 30 d |
| `fusion.entity` | resolver | 1 y |
| `world.delta` | ontology | 1 y |
| `assess.alert` | threat | 7 y |
| `decision.mission` | missions | 7 y |
| `decision.audit` | governor | **indefinite, hash-chained, never pruned** |
| `evidence.seal` | evidence | indefinite |
| `feeds.external` | feeds | 90 d |
| `ops.health` | all | 30 d |

- Every message is an `Envelope` (see proto): `{ event_id, property_id, occurred_at, recorded_at, provenance, payload, prev_hash, hash }`.
- **`decision.audit` is hash-chained.** A gap or a broken link is a P0 incident and locks the appliance out of physical actions until a human clears it.

### 7.2 Stores (all materialised views — rebuildable from the log)

| Concern | Store | Notes |
|---|---|---|
| World model | **Postgres 16** | Relational + recursive CTEs. Home scale does not need a graph DB. |
| Time-series | **TimescaleDB** | Device telemetry, battery, RF, sensor state. |
| Vectors | **pgvector** | Face/body/gait/vehicle embeddings + semantic event search. |
| Media | **MinIO, content-addressed** | The SHA-256 **is** the object key. Chain of custody becomes structural, not procedural. |

Encrypted at rest. Keys in the appliance TPM. **Any store can be dropped and rebuilt by replaying the log.** This is tested weekly in CI: `make rebuild-from-log` must produce a byte-identical ontology.

### 7.3 Retention

Policy per `ZoneClass`, set at onboarding, enforced by `evidence`:

| Zone class | Default retention |
|---|---|
| `PRIVATE` (bedrooms, bathrooms) | **No cameras. Sensors only.** |
| `INTERIOR` | 24 h rolling |
| `THRESHOLD` | 30 d |
| `GROUNDS`, `PERIMETER` | 90 d |
| Sealed evidence | indefinite, until owner destroys it |

---

## 8. Contracts

`proto/aegis/v1/aegis.proto` is normative. Generate for Rust (`prost`/`tonic`), Go (`protoc-gen-go`), Python (`grpcio-tools`), TypeScript (`ts-proto`). `make gen` does all four. **Hand-written DTOs are a build failure** — there is a lint for it.

Wire: gRPC + mTLS internally. gRPC-web / WebSocket via `bff` externally. Media via WebRTC.

---

## 9. Service specifications

Each service below states: **responsibility · consumes · produces · key algorithms · failure mode · acceptance test**.

### 9.1 `gateway` (Rust)

- **Responsibility.** Host DAL drivers. Normalise everything to `Envelope`. Stamp provenance. Discipline clocks. Reject anything without valid authority.
- **Consumes.** Devices and feeds, via drivers.
- **Produces.** `raw.device`, `ops.health`.
- **Key mechanics.**
  - **Clock discipline (critical).** Prefer **PTP** (IEEE 1588) where the device supports it; fall back to NTP with measured offset. Every event carries `captured_at` (device clock), `recorded_at` (appliance clock), and `clock_offset_ms` with a confidence. **Fusion quality is bounded by clock quality.** If `|offset| > 50 ms`, the device is flagged `DEGRADED` and its sightings are down-weighted in the resolver, not silently trusted.
  - **Attestation.** Devices with a secure element sign each payload. `attested = true` gates them into the higher-trust path. Unattested devices still work (that's how you land customers with existing gear) but their events cannot alone raise severity above 3.
  - **Backpressure.** Bounded queues, drop-oldest on `raw.device` telemetry, **never drop** on sighting/contact/alarm classes.
- **Failure mode.** A driver panics → supervised restart, device marked `FAULT`, operator notified. A driver cannot take down the gateway (drivers run out-of-process, `drivers/` are separate binaries speaking a local gRPC).
- **Acceptance.** `tools/dal-conformance` passes for every driver. Fuzz suite: malformed ONVIF, replayed frames, 5 s clock skew, and a device claiming an authority it doesn't have → all rejected, all logged, none crash.

### 9.2 `perception` (Python)

- **Responsibility.** Turn pixels and audio into `Sighting`s with embeddings.
- **Consumes.** `raw.device` (video/audio refs).
- **Produces.** `perception.sighting`.
- **Key mechanics.**
  - Detection: person / vehicle / animal / package / drone. Run at the edge where the device supports it (`CAP_EDGE_EMBEDDER`), else on the appliance GPU.
  - Per-sensor tracking: BoT-SORT-class (Kalman + IoU + appearance).
  - Embeddings: body re-ID, gait, vehicle, plate. **Face embeddings only for enrolled, consented persons** (§3.5).
  - Audio: glass-break, gunshot, raised voices, alarm tones. Cheap, high-value, widely under-used.
  - **Ground-plane projection.** Each camera is calibrated with a homography at install. Every sighting carries a site-coordinate `world_position` with a covariance. **Without this, cross-sensor fusion is impossible.** The install flow must capture it, and the console must show which cameras are uncalibrated.
- **Failure mode.** GPU degraded → fall back to a lightweight detector, raise confidence thresholds, mark the system `DEGRADED`, tell the operator. **Never silently degrade.**
- **Acceptance.** On the labelled corpus: person recall ≥ 0.97 at night on the perimeter set; vehicle plate read ≥ 0.90 at the gate; false-detection rate on the 30-night "empty estate" set ≤ 0.5/night/camera.

### 9.3 `resolver` (Rust)

- **Responsibility.** `Sighting → Track → Entity → Identity`.
- **Consumes.** `perception.sighting`.
- **Produces.** `fusion.entity`.
- **Key mechanics.**
  - Cross-sensor association cost:
    `C = w_s · d_mahalanobis(ground-plane position/velocity) + w_a · (1 − cos_sim(embedding)) + w_t · temporal_inconsistency`
    Hungarian assignment over a sliding window. Entity state tracked by an EKF in site coordinates. Start with `w_s=0.5, w_a=0.4, w_t=0.1`; tune against the golden corpus, **never against a live estate**.
  - Identity binding: gallery match vs enrolled `Person`/`Vehicle` above threshold `τ_known`; else attach to an existing unknown cluster or mint a new handle.
  - **Unknown handle persistence.** An unknown is a persisted embedding cluster with a stable ID (`unknown:` + 4 hex of the cluster key). Clusters live 180 days; a re-encounter refreshes them. Cross-property propagation of *embeddings + metadata only* (never media), encrypted, under owner authority, via `sync`.
  - **Expectation matching** happens here: an entity is stamped `expected = true` if it matches an active `Expectation` (identity, or vehicle plate, or a pre-registered one-time code presented at the gate).
- **Failure mode.** Ambiguous association → emit the entity with low confidence and *both* hypotheses attached. Never guess silently. The operator sees the ambiguity.
- **Acceptance.** On the golden corpus: ID-switch rate < 2%; cross-sensor handoff success ≥ 95%; **an unknown seen at two properties three days apart resolves to one handle in ≥ 90% of cases.**

### 9.4 `ontology` (Go)

- **Responsibility.** Materialise and serve the digital twin. The only service that writes Postgres.
- **Consumes.** `fusion.entity`, `raw.device`, admin RPCs.
- **Produces.** `world.delta` (a diff stream — this is what the console subscribes to).
- **API.** `GetTwin`, `StreamTwin`, `Query` (structured, read-only), `UpsertPerson`, `UpsertExpectation`, `SetPosture`, `GrantAccess`, `RevokeAccess`.
- **Failure mode.** Corrupt view → rebuild from log; serve stale-but-labelled during rebuild. Never serve silently-stale data.
- **Acceptance.** `make rebuild-from-log` produces a byte-identical twin. `StreamTwin` p99 delta latency < 120 ms.

### 9.5 `pol` — pattern of life (Python)

- **Responsibility.** Learn what normal is, so that abnormal is cheap to spot.
- **Consumes.** `world.delta`, `fusion.entity`.
- **Produces.** anomaly scores on `assess.*`.
- **Key mechanics.**
  - Baseline per `(zone, hour-of-week, object_class, identity_known)` → event-rate model (negative binomial; the variance matters more than the mean).
  - Trajectory typicality: learned traffic patterns on the site graph; score path deviation.
  - Dwell typicality: how long does anything normally stay here.
  - **Cold start.** 21-day learning window with an archetype prior. During it, `pol` runs in **shadow mode**: it scores, it logs, it does not raise severity. The console shows what it *would* have done. The operator's dismissals are the training signal.
- **Failure mode.** Model unavailable → `pol` contributes 0 and says so in the receipt. Threat scoring still works, just blunter.
- **Acceptance.** After 21 days on a pilot estate, `pol` correctly labels ≥ 90% of routine daily events as unremarkable, with zero suppression of any labelled true positive.

### 9.6 `threat` (Go)

- **Responsibility.** Turn signals into a severity 1–5 **with a receipt**.
- **Hard rule.** **The scorer is deterministic and explainable. No neural network sets severity.** The number that decides whether you wake the principal at 03:00 must be defensible line by line.

```
score = base(object_class, identity_known, zone_class)
      × posture_amplifier(posture, time_of_day)
      × (1 − expectation_discount(expected))
      + pol_anomaly_term
      + corroboration_term(mesh, multi-sensor)
      + dwell_term
      − known_benign_term
severity = bucket(score)   # thresholds are config, per property, versioned
```

Every term is emitted into `Alert.receipt` with its input, weight, and contribution. The console renders it. **The receipt is the feature** — it is what makes an operator trust the system and what makes an insurer accept the data.

- **Learning loop.** `Dismiss(alert_id, reason)` is a first-class RPC. Dismissal reasons are a closed enum plus free text; they retrain `pol` and tune thresholds **offline, in shadow, never live**.
- **Acceptance.** See §21 — this is the M3 kill gate.

### 9.7 `missions` (Rust)

- **Responsibility.** Playbook evaluation, mission lifecycle, asset tasking. The *only* client of `governor`.
- **Consumes.** `assess.alert`, operator commands via `bff`.
- **Produces.** `decision.mission`; asset commands **only when holding a valid `ActionGrant`**.
- **Key mechanics.**
  - Compile playbooks (§11) into a decision graph at load. Reject any playbook that fails `playbook-lint`.
  - A mission is an object with an objective, ROE, assets, actions, and a debrief. Missions are replayable.
  - Every action request → `governor.Authorize()` → `ALLOW | DENY | REQUIRE_APPROVAL`.
  - On `REQUIRE_APPROVAL`, surface to the console; the operator's hold-to-authorize produces an **operator signature** that goes back into a second `Authorize` call.
- **Failure mode.** Governor unreachable → **no physical actions**, mission stays in `OBSERVING`, operator alerted loudly. Fail-closed.
- **Acceptance.** Property test: no code path in `missions` can emit an asset command without a grant that verifies against the Governor's public key. Enforced by an architecture test *and* by the asset refusing it anyway (§13).

### 9.8 `governor` (Rust) — see §12. The heart.

### 9.9 `evidence` (Go)

- **Responsibility.** Chain of custody from the sensor to the courtroom.
- **Mechanics.**
  - Media is content-addressed; the SHA-256 is the key.
  - Devices with secure elements sign frames/segments at capture. That signature is preserved end to end.
  - On alert trigger, `seal_on_trigger` builds an `EvidencePackage`: a Merkle tree over media hashes + the contributing event IDs + the audit records, root signed by the appliance HSM key.
  - `Export` produces a portable bundle. `tools/evidence-verify` is a **standalone binary a third party can run** — police, insurer, opposing counsel — to verify the chain without trusting us. Ship it, document it, and make it the thing you demo.
- **Acceptance.** Tamper any byte of any artefact in an exported bundle → `evidence-verify` fails and names the artefact.

### 9.10 `feeds` (Go)

- **Responsibility.** External federation with authority enforcement.
- **Mechanics.** Each connector declares its `Authority` and an `authority_ref` (a consent record, a mesh agreement, a licence). **A connector with no valid authority ref cannot start.** Feed content is treated as **untrusted input** and is never passed to any component that can act (§16.5).
- **Connectors at v1.** Neighbour mesh · HOA · weather · wildfire/emergency alerts · ADS-B (a drone or aircraft loitering over the property is a real and unaddressed threat for this clientele) · AIS · public dispatch · threat-intel/doxxing monitoring.
- **Acceptance.** Revoking a mesh consent removes that node's data from the twin within 60 s and marks all derived alerts as `authority_revoked` without deleting the audit record.

### 9.11 `agent` (Python) — the LLM copilot

**This is the component most likely to be built wrong, so read carefully.**

- **Responsibility.** Briefings, natural-language query over the ontology, and *proposed* courses of action.
- **What it has.** Read-only tools: `query_ontology`, `search_events`, `summarize_incident`, `propose_coa`.
- **What it does not have.** Credentials. Any write path. Any ability to call `missions` or `governor`. **`propose_coa` returns a proposal object that a human must act on. It cannot execute.**
- **Prompt-injection.** External feed content, staff-entered notes, and any text from outside the trust boundary are **data, not instructions**. The agent runs with a tool set that *cannot act*, which means a successful injection yields a wrong answer, not a launched drone. **This is a design property, not a filter.** Do not rely on filtering.
- **Acceptance.** An architecture test asserts `py/agent` has no import path to any mutating client. A red-team suite plants injection payloads in mesh feed content and threat-intel text; the pass condition is that no tool call outside the read-only set is ever attempted.

### 9.12 `sync` (Go)

Cross-property federation (unknown handles, postures, alerts), encrypted backup, signed OTA with A/B rollback. **Never a dependency for local function** (A4).

### 9.13 `bff` (Go)

gRPC-web + WebSocket for the console/apps, WebRTC signalling for video. Enforces RBAC per persona. **The bff has no authority of its own** — it is a projection and a relay.

---

## 10. Device Abstraction Layer

The DAL is not merely an integration layer. **It is the requirements document for the hardware you will build.** Write it now; live inside it for two years with third-party gear that fails to meet it; then build devices that meet it perfectly.

### 10.1 Capabilities (not model numbers)

`VIDEO_SOURCE · AUDIO_SOURCE · MOTION_SENSOR · CONTACT_SENSOR · GLASSBREAK · ACCESS_POINT · ILLUMINATOR · ANNUNCIATOR · MOBILE_ASSET · NETWORK_SENSOR · EXTERNAL_FEED · EDGE_EMBEDDER · SIGNED_CAPTURE · PTP_CLOCK · LOCAL_BUFFER`

Third-party gear implements a subset. **Our hardware implements the superset.** Same codebase.

### 10.2 Driver contract

Each driver is a separate process speaking local gRPC to `gateway`:

```
rpc Discover()            → stream DeviceDescriptor    # capabilities, not models
rpc Subscribe(device_id)  → stream Envelope            # provenance-stamped at source
rpc Command(DeviceCommand)→ CommandAck                 # NEVER for assets — see §13
rpc Health()              → stream DeviceHealth
rpc Calibrate(...)        → CalibrationResult          # homography for cameras
```

**`Command` may not move an asset.** Asset motion goes through `asset-adapter` and requires an `ActionGrant`. A lint enforces that no driver imports the asset command types.

### 10.3 The five things you will wish for and never get from third-party hardware

*(This section is the hardware spec. Treat it as such.)*

1. **Signed frames at capture.** A secure element signs every frame or segment. Chain of custody begins at the sensor, not the server. For a clientele facing litigation, insurers, and prosecution, nobody else offers this.
2. **Hard time sync (PTP, not NTP).** Cross-sensor fusion is bounded by clock discipline. Sloppy timestamps silently destroy entity resolution and you will spend six months blaming the model. **The most underrated hardware requirement in the entire system.**
3. **Embeddings at the edge.** Devices emit vectors, not just pixels. Bandwidth collapses, privacy improves, latency drops, and the appliance GPU stops being the bottleneck.
4. **Survivability.** PoE + wireless mesh + cellular failover, onboard battery, local recording buffer that reconciles into the log when the link returns. **The wire will be cut. Design for it.**
5. **Robots expose a *mission* API, not a joystick API.** `goto · orbit · observe · follow · return`. Onboard autonomy handles navigation; the platform tasks *intent*. And the robot enforces geofence and ROE **onboard** — it must be *incapable* of the forbidden thing.

`tools/dal-conformance` is the executable form of this section. Your hardware ships when it passes.

---

## 11. Playbooks as code

Declarative, versioned, diffable, dry-runnable, and **regression-tested against recorded incidents before they can touch a live property**.

```yaml
playbook: perimeter_breach_night
version: 4
trigger:
  all:
    - entity.identity.known == false
    - entity.zone.class in [PERIMETER, GROUNDS]
    - posture in [AWAY, NIGHT]
    - entity.confidence >= 0.85
    - entity.expected == false
assess:
  cross_check: [expectations, mesh_sightings, vehicle_registry, pol_baseline]
roe:
  max_rung: HANDOFF
  geofence: property.geofence
  masks_offsite_optics: true
actions:
  - rung: OBSERVE
    autonomy: HUMAN_SUPERVISED       # 8 s abort window — the sweet spot
    asset: any_available_air
  - rung: ILLUMINATE
    autonomy: HUMAN_SUPERVISED
  - rung: ANNOUNCE
    autonomy: HUMAN_DIRECTED         # Governor enforces this regardless of what is written here
  - rung: SHADOW
    autonomy: HUMAN_DIRECTED
    stop_at: geofence
  - rung: HANDOFF
    autonomy: HUMAN_DIRECTED
    targets: [detail, law_enforcement]
notify:
  console: immediate
  owner: severity >= 4
evidence:
  seal_on_trigger: true
```

### 11.1 Rules

- `playbook-lint` **rejects** any playbook that requests an autonomy level above the compiled cap for a rung. A playbook cannot loosen safety. It can only tighten.
- Every playbook has a regression suite in `sim/scenarios/`. **A playbook that has not passed its suite cannot be deployed to a live property.** This is enforced in the deploy pipeline, not by convention.
- New playbooks and new versions run in **shadow mode for 30 days** against live traffic, scored against operator decisions, before they may authorize anything.

---

## 12. ★ The Autonomy Governor

Design it like flight software. Small, deterministic, no allocation surprises, no network calls to models, formally reviewable, fail-closed. **One engineer owns it. Every change is reviewed by two.**

### 12.1 Decision function

```
Authorize(req) -> Decision

INPUT  mission_id, rung, autonomy_requested, asset_id, asset_state,
       zone, trajectory_envelope, geofence, entity_confidence,
       posture, time, operator_credential?, operator_signature?,
       policy_version

OUTPUT ALLOW(ActionGrant) | REQUIRE_APPROVAL(reason) | DENY(reason)
       + AuditRecord  (always, on every path, including DENY)
```

### 12.2 Compiled-in invariants — no configuration may override

```
I1  rung >= ANNOUNCE  ⟹  operator_signature REQUIRED and VERIFIED, else REQUIRE_APPROVAL
I2  autonomy == HUMAN_ON_LOOP  ⟹  rung ∈ {OBSERVE, ILLUMINATE}
I3  trajectory_envelope ⊄ geofence  ⟹  DENY
I4  effective_cap = min(compiled_cap, policy_cap, playbook_cap)     # policy may only tighten
I5  entity_confidence < τ_rung  ⟹  REQUIRE_APPROVAL   (never auto-act on a weak signal)
I6  asset_state ∉ {READY, ON_STATION}  ⟹  DENY
I7  clock_skew or log-chain break detected  ⟹  DENY all physical actions
I8  governor degraded / policy unverifiable / in any doubt  ⟹  DENY
I9  there is no rung above HANDOFF; the enum has no such value and this function no such branch
```

### 12.3 The two-key model — the mechanism that makes doctrine cryptographic

An `ActionGrant` is a short-lived signed capability. **Nothing moves without one.**

```
ActionGrant {
  grant_id, mission_id, asset_id
  rung, autonomy
  geofence_hash                 # asset verifies this matches its loaded fence
  not_before, not_after         # ≤ 30 s TTL
  authorized_by                 # operator id, or rule id for autonomous rungs
  rule_version, policy_version
  governor_signature            # Ed25519, key in appliance HSM
  operator_signature            # REQUIRED for rung >= ANNOUNCE
}
```

- The **asset firmware holds the Governor's public key.** No valid grant → no motion. Not "refuses politely" — *cannot*.
- For `rung >= ANNOUNCE`, the grant carries **two** signatures. Even a fully compromised Governor cannot make a drone speak to a person without an operator's key.
- A compromised console, a compromised `missions` service, and a compromised network are each, independently, insufficient to move an asset.

**This is the single strongest idea in the architecture. Build it first, in M4, before any robot exists.**

### 12.4 Test vectors (must exist as literal test files)

| # | Scenario | Expected |
|---|---|---|
| G-01 | `OBSERVE`, on-loop, conf 0.91, inside fence | `ALLOW` + grant, 1 signature |
| G-02 | `ANNOUNCE`, no operator signature | `REQUIRE_APPROVAL` |
| G-03 | `ANNOUNCE`, forged operator signature | `DENY` + P0 security event |
| G-04 | `OBSERVE`, trajectory clips 2 m outside fence | `DENY` |
| G-05 | Policy file requests `HUMAN_ON_LOOP` for `ANNOUNCE` | policy **rejected at load**; governor runs on last-good policy |
| G-06 | Log-chain break detected | all physical `DENY` until human clears |
| G-07 | Grant replayed 31 s later | asset rejects (TTL) |
| G-08 | Grant with mismatched `geofence_hash` | asset rejects |
| G-09 | Any request for a rung value > `HANDOFF` | fails to deserialise; type does not exist |
| G-10 | Governor unreachable | `missions` denies, observes, alerts operator |

Property-based tests (`proptest`) must hold I1–I9 over **randomly generated** requests. This is not optional coverage; it is the reason the product is insurable.

---

## 13. Asset control protocol

- `missions` never talks to a robot. It talks to `asset-adapter`, and only with a grant.
- `asset-adapter` verifies the grant signature, then translates the rung into a **mission-level** command: `goto · orbit · observe · follow · return · recall`. Never a joystick stream.
- The asset verifies the grant **again** against the Governor's public key it holds onboard, checks `geofence_hash` against its loaded fence, and checks the TTL.
- Onboard: an independent **watchdog** forces return-to-dock on fence breach, link loss > 5 s, low battery, or grant expiry. The watchdog is separate from the flight stack and cannot be commanded off.
- **Optics masking is done in the encoder on the asset**, keyed to pose. The masked frames are the recorded frames.
- `recall` requires no grant. Stopping is always allowed. **Every dangerous verb needs a key; the safe verb needs none.**

**Before any asset flies over a real property:** 1000 SITL missions in `sim/sitl` with zero geofence breaches, then 200 supervised live flights on an empty test site with zero breaches. No exceptions, no schedule pressure.

---

## 14. Interfaces

### 14.1 Console (operator)

Dense, keyboard-first, map-primary. A working prototype exists (`operator-console.jsx`) — treat it as the design contract.

- **Live site map** is the home screen, not a camera grid. Entities move on it; cameras are one keystroke from any entity.
- **Triage queue** — fused, severity-ranked, each with its **fusion receipt** (the exact events, weights and terms that produced the number).
- **Escalation ladder** with visible Governor state. `OBSERVE`/`ILLUMINATE` execute on click; **`ANNOUNCE` and above require press-and-hold**, which produces the operator signature. The UI makes the doctrine physical.
- **Supervised-launch abort window** — 8 s, prominent, one key.
- **Entity workbench** — pivot from an entity to every appearance, zone, property, and prior encounter.
- **Watch tape** — the append-only log, visible at all times. Every panel above it is a projection of it, and the operator can see that.
- **Shift handoff report** — auto-generated: anomalies, open items, dismissals with reasons. Nobody builds this and every professional detail needs it.

### 14.2 Owner app

Austere by law. Every proposed feature must **reduce** the principal's cognitive load or it is cut.

- One status glyph: all properties secure — where green means *verified* green, not *no alerts yet*.
- Daily briefing, agent-written, one paragraph.
- **Approval requests** — the only interruption. "Operator requests `ANNOUNCE`. Approve / Deny / Call me."
- People & access: grant, revoke, expiring guest passes.
- Panic. Lockdown.

### 14.3 Staff app

Their zones, their schedule, and **pre-registration of expected visitors** — which is the highest-leverage false-alarm feature in the product and lives in the least glamorous app.

### 14.4 Decision-rights matrix

Configured at onboarding. The subtle heart of the product.

| Severity | Who is woken |
|---|---|
| 1–3 | Operator absorbs silently |
| 4 | Operator + security chief |
| 5 | Operator + chief + **owner** + detail + (optionally) law enforcement, simultaneously |

*The owner buys silence. The console absorbs the noise.*

---

## 15. Security architecture

1. **Zero trust internally.** Assume a compromised camera. Every device is attested, least-privileged, network-segmented. **A camera cannot task a drone.** Ever.
2. **Key hierarchy.** Appliance HSM root → Governor signing key (Ed25519) → per-operator credential keys (hardware token) → per-device identity keys (secure element). Rotation is an audited event.
3. **The platform is itself a high-value target because of who buys it.** Red team from month one. Treat the console as critical infrastructure, not a web app.
4. **Physical + digital fusion.** Network intrusion attempts, doxxing, address leaks, and threat chatter are events in the same ontology and can raise the estate's `Posture` automatically. For this clientele the two domains are one domain.
5. **Untrusted input boundary.** External feeds, staff notes, uploaded documents → data, never instructions. The LLM's tool set cannot act (§9.11), which converts prompt injection from a catastrophe into a wrong answer.
6. **Data sovereignty is the sale.** On-prem by default. The owner can export, revoke, and destroy. **Every access to their data — including by our own staff — is an `AuditRecord` the owner can read.** Make that visible in the owner app. It is worth more than any feature.

---

## 16. Reliability & degradation ladder

| Level | Condition | Behaviour |
|---|---|---|
| L0 | Nominal | Full function |
| L1 | Cloud unreachable | **Full local function.** No cross-property, no OTA. Operator informed, not alarmed. |
| L2 | GPU degraded | Lightweight detectors, raised thresholds, system marked `DEGRADED` in the UI |
| L3 | **Governor unreachable** | **No physical actions.** Observe, record, alert loudly. Fail-closed. |
| L4 | Appliance down | Devices keep recording to local buffers; alarm panel fallback; operator paged over cellular; buffers reconcile into the log on recovery |
| L5 | Power loss | UPS graceful shutdown; devices on battery; cellular alerting |

**Black-start is tested in production, monthly, by unplugging the WAN.** If it is not tested, it does not work.

---

## 17. Simulation, replay & assurance

The harness is not a nice-to-have. **It is the precondition for shipping autonomous physical response.**

- **Deterministic replay.** Because domain logic never calls `now()` (A7), any recorded incident replays byte-identically. `make replay INCIDENT=A-2291`.
- **Scenario DSL.** `sim/scenarios/*.yaml` scripts entities, sensors, weather, and operator actions. Every playbook has a suite.
- **SITL.** Assets fly in simulation (PX4/Gazebo) against the real Governor and the real `asset-adapter`. The Governor does not know it is a simulation.
- **Shadow mode.** New models and playbooks run silently against live traffic for 30 days, scored against operator decisions, before they can authorize anything.
- **Adversarial suite.** Spoofed device identity · replayed frames · 5 s clock skew · a camera claiming to be a drone · prompt injection through mesh feed content · a forged grant · a replayed grant. All must fail closed and all must be logged.

---

## 18. Observability

OpenTelemetry throughout. The dashboards that matter:

- **`signal_to_decision_seconds`** — the north star. Histogram, p50/p95, per property. Everything else is secondary.
- `false_alert_rate` per estate per night (the M3 gate metric)
- `governor_decisions{outcome}` — and any `DENY` for `forged_signature` is a P0 page
- `geofence_breaches` — **must be zero, forever**
- `log_chain_integrity` — any break is a P0
- `device_clock_offset_ms` p99 — the silent killer of fusion quality

---

## 19. Deployment

- **Appliance.** Fanless industrial x86 + NVIDIA RTX A2000-class (or Jetson Orin for smaller sites). TPM 2.0. Dual NIC. UPS. NVMe RAID-1. 10 GbE to the camera VLAN.
- **Runtime.** k3s. Everything containerised. **A/B partitions with signed OTA and automatic rollback** on health-check failure.
- **Secrets.** TPM-sealed. The Governor's signing key **never leaves the HSM** and is never present in a container image or an env var.
- **Dev.** `docker compose up` + the simulator gives a full working system with zero hardware. **An engineer must be able to run the entire product, including a simulated drone, on a laptop, on day one.** If that is ever untrue, fix it before writing another feature.

---

## 20. Testing strategy

| Layer | Method |
|---|---|
| Unit | Standard |
| **Governor** | **Property-based (`proptest`) over I1–I9 + literal test vectors G-01…G-10.** Highest bar in the repo. |
| Contracts | Proto compatibility checks in CI; breaking changes require an ADR |
| Fusion | Golden corpus replay with labelled ground truth |
| Playbooks | Regression suite per playbook; deploy blocked without a pass |
| Assets | SITL (1000 missions) → live test site (200 flights) → customer site |
| Architecture | Lints: no LLM in an authorization path; no service writes Postgres but `ontology`; no driver imports asset commands; no hand-written DTOs |
| Adversarial | §17 suite, run nightly |
| Black-start | Monthly, in production, by unplugging |

---

## 21. Milestones and acceptance gates

Each milestone ships to a real estate. Nothing is "done" in a lab.

| M | Deliverable | Definition of done |
|---|---|---|
| **M0** | Repo, protos, event log, CI, simulator skeleton, `CLAUDE.md` | `docker compose up` runs a synthetic estate end to end with zero hardware |
| **M1** | `gateway` + `ontology` + console timeline & map | Third-party cameras/sensors ingested; unified timeline; incident replay works. **No AI yet.** Ship the *unified truth* first — it is already better than what they have. |
| **M2** | `perception` + `resolver` | Entity resolution beats a single smart camera on the golden corpus (§9.3 metrics) |
| **M3** | `pol` + `Expectation` + `threat` receipts | **★ KILL GATE ★** |
| **M4** | Playbooks + **Governor** + two-key grants + fixed responders (lights, audio, locks) | Autonomous graduated response **with zero robots**. G-01…G-10 pass. Property tests hold. |
| **M5** | `evidence` + owner app + staff app | A third party can verify a sealed package with `evidence-verify` and no trust in us |
| **M6** | Ground robot (indoor) | No airspace regulator involved. **Probably your real v1 responder — do not let the drone's glamour reorder this.** |
| **M7** | Drone tier (supervised launch) | 1000 SITL + 200 live flights, **zero geofence breaches**. Regulatory review completed *at the start of this milestone*, not the project. |
| **M8** | Federation: mesh, external feeds, cross-property | An unknown handle propagates between two estates under owner authority |
| **M9** | Own hardware | Passes `tools/dal-conformance`, which you have been living inside for two years |

### ★ The M3 kill gate

On **20 pilot estates, over 60 nights**, measured against the baseline of the devices' own native alerting:

- **≥ 80% reduction** in alerts an operator must action
- **≤ 1 missed true positive** across the entire corpus
- Every alert carries a receipt an operator says they trust

**If this gate fails, the fusion thesis is wrong, and no amount of drone engineering will save the product. Stop. Do not build M6 or M7. Re-examine the thesis or return the money.**

This gate is the most valuable paragraph in this document.

---

## 22. Risk register

| Risk | Mitigation |
|---|---|
| Fusion is not meaningfully better than one good camera | The M3 kill gate. Measured, not asserted. |
| Drone autonomy blocked by airspace regulation | Ground robot is the fallback responder (M6 before M7, deliberately). Regulatory review at M7 start. |
| First lawsuit over a neighbour's privacy | Triple-enforced geofence; optics masked in the encoder; every frame's pose logged. |
| Hardware capex kills the company | Software-first. White-label the first drone. Build hardware only after the DAL spec is proven (M9). |
| An operator or an insider abuses the system | Every data access is an audit record the **owner** can read. The owner audits us. |
| We become the story | The authority layer. If we cannot show the right to a signal, we do not have it. |
| The LLM does something | It has no credentials and no write path. Injection yields a wrong sentence, not a launched drone. |

---

## 23. North star

Not alerts sent. Not uptime. Not devices connected.

> ## Median seconds from first sensor signal to a human decision made with full context.

Palantir's entire value was compressing intelligence-to-decision time. Ours is identical — for the people with the most to lose.

Instrument it in M1. Put it on the wall. Optimise nothing else until it is the best number in the industry.
