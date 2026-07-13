-- ============================================================================
--  AEGIS — Postgres schema.
--
--  EVERY TABLE HERE IS A MATERIALISED VIEW OVER THE EVENT LOG.
--  Nothing in this database is authoritative. `make rebuild-from-log` must
--  reproduce it byte-identically, and CI checks that weekly. If you are
--  tempted to store something here that cannot be derived from the log,
--  you have misunderstood axiom A1 — go and read the spec.
--
--  Only services/ontology writes to this database. There is a lint.
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS vector;      -- pgvector
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS postgis;     -- site geometry

-- ── places ─────────────────────────────────────────────────────────────────
CREATE TABLE property (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  boundary      GEOMETRY(POLYGON) NOT NULL,
  geofence      GEOMETRY(POLYGON) NOT NULL,   -- inset from boundary; assets may not cross
  geofence_hash BYTEA NOT NULL,               -- assets verify grants against this
  timezone      TEXT NOT NULL,
  jurisdiction  TEXT NOT NULL,                -- drives the regulatory policy file.
                                              -- NEVER hard-code a regulatory rule in code.
  CONSTRAINT geofence_within_boundary CHECK (ST_Within(geofence, boundary))
);

CREATE TYPE zone_class AS ENUM
  ('PERIMETER','GROUNDS','THRESHOLD','INTERIOR','PRIVATE','SAFE_ROOM');

CREATE TABLE zone (
  id             TEXT PRIMARY KEY,
  property_id    TEXT NOT NULL REFERENCES property(id),
  name           TEXT NOT NULL,
  area           GEOMETRY(POLYGON) NOT NULL,
  class          zone_class NOT NULL,
  retention_days INT NOT NULL DEFAULT 30
);

-- ── devices & assets ───────────────────────────────────────────────────────
CREATE TYPE device_state AS ENUM ('READY','DEGRADED','FAULT','OFFLINE');

CREATE TABLE device (
  id           TEXT PRIMARY KEY,
  property_id  TEXT NOT NULL REFERENCES property(id),
  zone_id      TEXT REFERENCES zone(id),
  vendor       TEXT, model TEXT,
  capabilities TEXT[] NOT NULL,
  pose         JSONB,
  calibrated   BOOLEAN NOT NULL DEFAULT FALSE,  -- no homography → no cross-sensor fusion.
  homography   BYTEA,                            -- the console MUST surface uncalibrated cameras
  state        device_state NOT NULL DEFAULT 'OFFLINE',
  attested     BOOLEAN NOT NULL DEFAULT FALSE,   -- unattested devices cannot alone raise sev > 3
  battery      REAL
);

-- A camera may not exist in a PRIVATE zone. Enforced here as well as at install.
CREATE OR REPLACE FUNCTION no_cameras_in_private_zones() RETURNS TRIGGER AS $$
BEGIN
  IF 'VIDEO_SOURCE' = ANY(NEW.capabilities)
     AND (SELECT class FROM zone WHERE id = NEW.zone_id) = 'PRIVATE' THEN
    RAISE EXCEPTION 'A VIDEO_SOURCE may not be placed in a PRIVATE zone (spec §7.3)';
  END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_no_cameras_in_private
  BEFORE INSERT OR UPDATE ON device
  FOR EACH ROW EXECUTE FUNCTION no_cameras_in_private_zones();

CREATE TYPE asset_kind  AS ENUM ('AIR','GROUND','FIXED_RESPONDER');
CREATE TYPE asset_state AS ENUM ('DOCKED','LAUNCHING','ENROUTE','ON_STATION','RETURNING','ASSET_FAULT');

CREATE TABLE asset (
  id              TEXT PRIMARY KEY,
  property_id     TEXT NOT NULL REFERENCES property(id),
  kind            asset_kind NOT NULL,
  dock_id         TEXT,
  state           asset_state NOT NULL DEFAULT 'DOCKED',
  battery         REAL,
  permitted_rungs TEXT[] NOT NULL,   -- also enforced ONBOARD. The asset must be
  geofence_hash   BYTEA NOT NULL,    -- INCAPABLE of the rest, not merely unwilling.
  governor_pubkey BYTEA NOT NULL     -- no valid grant → no motion
);

-- ── people, access, expectations ───────────────────────────────────────────
CREATE TYPE person_role AS ENUM ('PRINCIPAL','FAMILY','STAFF','DETAIL','VENDOR','GUEST');

CREATE TABLE person (
  id                TEXT PRIMARY KEY,
  property_id       TEXT NOT NULL REFERENCES property(id),
  display_name      TEXT NOT NULL,
  role              person_role NOT NULL,
  flagged           BOOLEAN NOT NULL DEFAULT FALSE,
  flag_reason       TEXT,
  consent_biometric BOOLEAN NOT NULL DEFAULT FALSE   -- no consent → no template. Ever.
);

-- Enrolment-only, consented, local-only. Never matched against external
-- databases. Deletion is itself an audit record. (spec §3.5)
CREATE TABLE embedding (
  id         TEXT PRIMARY KEY,
  subject_id TEXT NOT NULL,            -- person: | vehicle: | unknown:
  kind       TEXT NOT NULL,            -- body | gait | face | vehicle | plate
  vec        VECTOR(512) NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ               -- unknown clusters live 180 d; refreshed on re-encounter
);
CREATE INDEX ON embedding USING hnsw (vec vector_cosine_ops);

CREATE TABLE access_grant (
  id         TEXT PRIMARY KEY,
  person_id  TEXT NOT NULL REFERENCES person(id),
  zone_ids   TEXT[] NOT NULL,
  starts_at  TIMESTAMPTZ NOT NULL,
  ends_at    TIMESTAMPTZ,
  granted_by TEXT NOT NULL
);

-- The single largest false-alarm reduction in the system, and the least
-- glamorous table in the schema. It lives in the staff app. (spec §6.2)
CREATE TABLE expectation (
  id            TEXT PRIMARY KEY,
  property_id   TEXT NOT NULL REFERENCES property(id),
  person_id     TEXT REFERENCES person(id),
  vehicle_id    TEXT,
  onetime_code  TEXT,
  zone_ids      TEXT[] NOT NULL,
  starts_at     TIMESTAMPTZ NOT NULL,
  ends_at       TIMESTAMPTZ NOT NULL,
  rrule         TEXT,
  registered_by TEXT NOT NULL,
  note          TEXT
);
CREATE INDEX ON expectation (property_id, starts_at, ends_at);

CREATE TYPE posture AS ENUM ('NOMINAL','AWAY','NIGHT','ELEVATED','LOCKDOWN');
CREATE TABLE property_posture (
  property_id TEXT PRIMARY KEY REFERENCES property(id),
  posture     posture NOT NULL DEFAULT 'NOMINAL',
  set_by      TEXT NOT NULL,
  set_at      TIMESTAMPTZ NOT NULL
);

-- ── identity: the durable handle for a stranger ────────────────────────────
CREATE TABLE identity (
  id                  TEXT PRIMARY KEY,   -- person:ana | vehicle:8XJ-4410 | unknown:7F3A
  known               BOOLEAN NOT NULL,
  person_id           TEXT REFERENCES person(id),
  vehicle_id          TEXT,
  encounter_count     INT NOT NULL DEFAULT 1,
  first_seen          TIMESTAMPTZ NOT NULL,
  last_seen           TIMESTAMPTZ NOT NULL,
  seen_at_property_ids TEXT[] NOT NULL DEFAULT '{}'   -- cross-property. This is the product.
);

CREATE TABLE entity (
  id              TEXT PRIMARY KEY,
  property_id     TEXT NOT NULL REFERENCES property(id),
  class           TEXT NOT NULL,
  identity_id     TEXT REFERENCES identity(id),
  alternate_identity_ids TEXT[] DEFAULT '{}',  -- ambiguity is carried, never resolved silently
  position        GEOMETRY(POINT),
  current_zone_id TEXT REFERENCES zone(id),
  dwell_seconds   REAL NOT NULL DEFAULT 0,
  confidence      REAL NOT NULL,
  expected        BOOLEAN NOT NULL DEFAULT FALSE,
  expectation_id  TEXT REFERENCES expectation(id),
  first_seen      TIMESTAMPTZ NOT NULL,
  last_seen       TIMESTAMPTZ NOT NULL
);

-- ── assessment ─────────────────────────────────────────────────────────────
CREATE TYPE alert_state AS ENUM ('OPEN','ACKED','ACTIONED','DISMISSED');

CREATE TABLE alert (
  id                     TEXT PRIMARY KEY,
  property_id            TEXT NOT NULL REFERENCES property(id),
  severity               INT NOT NULL CHECK (severity BETWEEN 1 AND 5),
  threat_score           DOUBLE PRECISION NOT NULL,
  entity_id              TEXT REFERENCES entity(id),
  zone_id                TEXT REFERENCES zone(id),
  contributing_event_ids TEXT[] NOT NULL,
  receipt                JSONB NOT NULL,   -- every term, its input, weight and contribution.
                                           -- THE RECEIPT IS THE FEATURE. An operator who
                                           -- cannot see WHY will not trust the system.
  pol_note               TEXT,
  mesh_corroborations    TEXT[],
  playbook_id            TEXT,
  mission_id             TEXT,
  state                  alert_state NOT NULL DEFAULT 'OPEN',
  dismiss_reason         TEXT,
  dismiss_note           TEXT,
  evidence_package_id    TEXT,
  created_at             TIMESTAMPTZ NOT NULL
);
CREATE INDEX ON alert (property_id, state, severity DESC, created_at DESC);

-- ── response ───────────────────────────────────────────────────────────────
CREATE TYPE mission_state AS ENUM ('OPENED','OBSERVING','ENGAGED','HANDED_OFF','CLOSED','ABORTED');

CREATE TABLE mission (
  id          TEXT PRIMARY KEY,
  property_id TEXT NOT NULL REFERENCES property(id),
  alert_id    TEXT REFERENCES alert(id),
  playbook_id TEXT NOT NULL,
  objective   TEXT NOT NULL,
  roe         JSONB NOT NULL,
  asset_ids   TEXT[] NOT NULL DEFAULT '{}',
  state       mission_state NOT NULL DEFAULT 'OPENED',
  debrief     TEXT,
  opened_at   TIMESTAMPTZ NOT NULL,
  closed_at   TIMESTAMPTZ
);

CREATE TABLE action (
  id                  TEXT PRIMARY KEY,
  mission_id          TEXT NOT NULL REFERENCES mission(id),
  rung                TEXT NOT NULL,
  autonomy            TEXT NOT NULL,
  asset_id            TEXT REFERENCES asset(id),
  authorized_by       TEXT,      -- an operator id, or a rule id. NEVER an LLM.
  authorized_at       TIMESTAMPTZ,
  governor_decision_id TEXT NOT NULL,
  grant_id            TEXT,
  state               TEXT NOT NULL,
  abort_reason        TEXT,
  -- Rungs at or above ANNOUNCE require a human signature. This constraint is a
  -- belt-and-braces echo of Governor invariant I1; the Governor is the real
  -- enforcement. If this constraint ever fires, something upstream is broken
  -- and it is a P0. (spec §12.2)
  CONSTRAINT human_required_above_illuminate CHECK (
    rung IN ('OBSERVE','ILLUMINATE') OR authorized_by IS NOT NULL
  )
);

-- ── accountability ─────────────────────────────────────────────────────────
-- Hash-chained. Never pruned. A break locks the appliance out of physical
-- actions until a human clears it. This table is the reason the product is
-- insurable and the reason it is defensible in court.
CREATE TABLE audit_record (
  id                    TEXT PRIMARY KEY,
  property_id           TEXT NOT NULL REFERENCES property(id),
  subject_id            TEXT NOT NULL,
  actor_id              TEXT NOT NULL,
  action                TEXT NOT NULL,
  rule_version          TEXT,
  confidence_at_decision REAL,
  evidence_event_ids    TEXT[],
  at                    TIMESTAMPTZ NOT NULL,
  prev_hash             BYTEA NOT NULL,
  hash                  BYTEA NOT NULL UNIQUE
);
CREATE INDEX ON audit_record (property_id, at DESC);

CREATE TABLE evidence_package (
  id               TEXT PRIMARY KEY,
  property_id      TEXT NOT NULL REFERENCES property(id),
  incident_id      TEXT NOT NULL,
  media_hashes     BYTEA[] NOT NULL,
  event_ids        TEXT[] NOT NULL,
  audit_record_ids TEXT[] NOT NULL,
  merkle_root      BYTEA NOT NULL,
  signature        BYTEA NOT NULL,
  sealed_at        TIMESTAMPTZ NOT NULL,
  sealed_by        TEXT NOT NULL
);

-- ── feeds: no authority reference, no ingestion ────────────────────────────
CREATE TABLE feed (
  id            TEXT PRIMARY KEY,
  property_id   TEXT NOT NULL REFERENCES property(id),
  kind          TEXT NOT NULL,     -- mesh | weather | wildfire | adsb | ais | dispatch | threatintel
  authority     TEXT NOT NULL,
  authority_ref TEXT NOT NULL,     -- consent record / contract / mesh agreement
  enabled       BOOLEAN NOT NULL DEFAULT TRUE,
  revoked_at    TIMESTAMPTZ,
  -- A connector with no authority reference cannot start. This is the constraint
  -- that keeps the company alive. (spec §3.4)
  CONSTRAINT authority_ref_required CHECK (length(authority_ref) > 0)
);

-- ── telemetry (Timescale) ──────────────────────────────────────────────────
CREATE TABLE device_telemetry (
  time             TIMESTAMPTZ NOT NULL,
  device_id        TEXT NOT NULL,
  battery          REAL,
  rssi             REAL,
  clock_offset_ms  INT,      -- p99 of this is the silent killer of fusion quality.
  state            TEXT      -- put it on the wall. (spec §18)
);
SELECT create_hypertable('device_telemetry','time');

-- ── the north star ─────────────────────────────────────────────────────────
-- Median seconds from first sensor signal to a human decision made with full
-- context. Instrument it in M1. Optimise nothing else until it is the best
-- number in the industry. (spec §23)
CREATE TABLE decision_latency (
  time              TIMESTAMPTZ NOT NULL,
  property_id       TEXT NOT NULL,
  alert_id          TEXT NOT NULL,
  first_signal_at   TIMESTAMPTZ NOT NULL,
  human_decision_at TIMESTAMPTZ NOT NULL,
  seconds           REAL NOT NULL
);
SELECT create_hypertable('decision_latency','time');
