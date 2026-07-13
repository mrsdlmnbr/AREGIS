// Package sync is the cross-property federation service (spec §9.12):
// unknown-handle federation, encrypted backup, and signed OTA with A/B
// rollback.
//
// AXIOM A4 — the house works with the wire cut. Sync is NEVER a dependency
// for local function. Perception, fusion, playbooks, response and recording
// run entirely on-premise with the WAN down; this service only ever adds
// (cross-property context, offsite durability, updates) and its absence
// must degrade to exactly L1 of the reliability ladder (spec §16): full
// local function, operator informed, not alarmed. Any future change that
// makes a local code path wait on sync is a bug by definition.
//
// M0 scope: typed job skeletons only. Every Run returns
// ErrNotImplementedInM0. The real implementations land in M8 (federation)
// and the deploy milestones (backup, OTA).
package sync

import (
	"errors"
	"time"
)

// ErrNotImplementedInM0 is returned by every job in this package. M0 ships
// the shapes so the milestone plan (spec §21) is visible in the tree; it
// does not ship the behaviour.
var ErrNotImplementedInM0 = errors.New("sync: not implemented in M0 (lands in M8+, spec §9.12)")

// FederateUnknownHandlesJob propagates durable unknown-identity handles
// (spec §6.1: "unknown:7F3A") between properties under the same ownership.
// The unknown at the gate on Tuesday and the unknown at the Aspen property
// on Friday resolve to the same handle. Runs only under explicit owner
// authority; embeddings travel encrypted and never leave the appliances in
// the clear (spec §3.5).
type FederateUnknownHandlesJob struct {
	SourcePropertyID string
	TargetPropertyID string
	HandleIDs        []string  // identity ids, all of the "unknown:" form
	AuthorityRef     string    // owner authorisation record — empty must refuse (spec §3.4)
	RequestedAt      time.Time // time is an input (A7)
}

func (j FederateUnknownHandlesJob) Run() error { return ErrNotImplementedInM0 }

// EncryptedBackupJob replicates the property's event log and stores to
// offsite storage, encrypted on the appliance before any byte leaves it.
// Keys stay in the appliance TPM (spec §7.2); the offsite copy is durable
// ciphertext, nothing more.
type EncryptedBackupJob struct {
	PropertyID string
	// Topics to replicate; decision.audit is always included — the
	// hash-chained audit log is never pruned and never left un-replicated.
	Topics      []string
	Destination string    // offsite bucket URN
	StartedAt   time.Time // time is an input (A7)
}

func (j EncryptedBackupJob) Run() error { return ErrNotImplementedInM0 }

// SignedOTAJob applies a signed over-the-air update to the inactive slot of
// an A/B partition scheme and switches on verified boot, rolling back
// automatically on failure (spec §9.12, deploy/). An image whose signature
// does not verify is refused before a byte is written. OTA is cloud-fed and
// therefore, per A4, its unavailability costs nothing locally (L1: no OTA,
// full function).
type SignedOTAJob struct {
	ApplianceID string
	ImageDigest string // content address of the image
	Signature   []byte // release-key signature over the digest
	TargetSlot  string // "A" or "B" — always the inactive slot
	ScheduledAt time.Time
}

func (j SignedOTAJob) Run() error { return ErrNotImplementedInM0 }
