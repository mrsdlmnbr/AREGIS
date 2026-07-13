// Package twin materialises the digital twin as a pure fold over the event
// log (spec §9.4, axiom A1: the log is the truth — the twin is a projection).
//
// Everything here is deterministic by construction:
//   - Apply is a pure state transition: (state, envelope) → state. It reads
//     no clock (axiom A7), no environment, no store.
//   - Rebuild folds from empty. `make rebuild-from-log` (M0 stand-in:
//     cmd/rebuild-check) must produce a byte-identical twin, forever.
//   - MarshalCanonical sorts every repeated field by id and marshals with
//     Deterministic: true, so two independent folds of the same log compare
//     byte for byte.
//
// Unknown payload types are a NO-OP with a counter, never an error: the log
// outlives the code, and a replay of a year-old log through today's binary
// must not fail because a later milestone added a payload kind this fold
// does not know. Same for undecodable payloads — counted, surfaced, skipped.
// A fold that can error is a fold that can leave the twin silently partial
// (spec §9.4 failure mode: never serve silently-stale data).
package twin

import (
	"sort"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"google.golang.org/protobuf/proto"
)

// Payload type strings as they appear in Envelope.payload_type. The
// convention (shared with crates/aegis-common) is the fully-qualified proto
// message name.
const (
	PayloadEntity      = "aegis.v1.Entity"            // fusion.entity — entity upsert
	PayloadPosture     = "aegis.v1.SetPostureRequest" // posture change
	PayloadDevice      = "aegis.v1.Device"            // raw.device — device state change
	PayloadAlert       = "aegis.v1.Alert"             // assess.alert — alert opened/updated
	PayloadExpectation = "aegis.v1.Expectation"       // expectation upsert
)

// Twin is the in-memory materialised world model. Keys are the object ids;
// the map form makes upserts O(1) and forces MarshalCanonical to impose the
// canonical (sorted) order explicitly rather than trusting insertion order.
type Twin struct {
	Property     *aegisv1.Property
	Posture      aegisv1.Posture
	Zones        map[string]*aegisv1.Zone
	Devices      map[string]*aegisv1.Device
	Assets       map[string]*aegisv1.Asset
	Entities     map[string]*aegisv1.Entity
	OpenAlerts   map[string]*aegisv1.Alert
	Expectations map[string]*aegisv1.Expectation

	// UnknownPayloads counts skipped envelopes per payload_type. Logs
	// outlive code: an unknown kind is data for the operator ("this twin
	// was built by a binary that did not understand N events"), not an
	// error. Not part of the canonical marshal.
	UnknownPayloads map[string]uint64
	// DecodeFailures counts envelopes whose payload_type was known but
	// whose bytes did not decode. Counted and skipped for the same reason.
	DecodeFailures uint64
}

// New returns the empty twin — the fold's identity element.
func New() *Twin {
	return &Twin{
		Zones:           map[string]*aegisv1.Zone{},
		Devices:         map[string]*aegisv1.Device{},
		Assets:          map[string]*aegisv1.Asset{},
		Entities:        map[string]*aegisv1.Entity{},
		OpenAlerts:      map[string]*aegisv1.Alert{},
		Expectations:    map[string]*aegisv1.Expectation{},
		UnknownPayloads: map[string]uint64{},
	}
}

// Apply folds one envelope into the state. It is total: it never returns an
// error, because a partial fold is worse than a counted skip (see package
// doc). Last-write-wins per id; per-key order is the log's per-key order,
// which the event log guarantees (spec §7.1: topics ordered per key).
func Apply(state *Twin, env *aegisv1.Envelope) {
	if env == nil {
		state.DecodeFailures++
		return
	}
	switch env.GetPayloadType() {
	case PayloadEntity:
		m := &aegisv1.Entity{}
		if proto.Unmarshal(env.GetPayload(), m) != nil {
			state.DecodeFailures++
			return
		}
		state.Entities[m.GetId()] = m

	case PayloadPosture:
		m := &aegisv1.SetPostureRequest{}
		if proto.Unmarshal(env.GetPayload(), m) != nil {
			state.DecodeFailures++
			return
		}
		state.Posture = m.GetPosture()

	case PayloadDevice:
		m := &aegisv1.Device{}
		if proto.Unmarshal(env.GetPayload(), m) != nil {
			state.DecodeFailures++
			return
		}
		state.Devices[m.GetId()] = m

	case PayloadAlert:
		m := &aegisv1.Alert{}
		if proto.Unmarshal(env.GetPayload(), m) != nil {
			state.DecodeFailures++
			return
		}
		// The twin projects *open* alerts (Twin.open_alerts). OPEN, ACKED
		// and ACTIONED alerts are still live for the console; a DISMISSED
		// alert leaves the projection. The alert's history stays in the
		// log — the twin is a view, not a record (A1).
		if m.GetState() == aegisv1.AlertState_DISMISSED {
			delete(state.OpenAlerts, m.GetId())
		} else {
			state.OpenAlerts[m.GetId()] = m
		}

	case PayloadExpectation:
		m := &aegisv1.Expectation{}
		if proto.Unmarshal(env.GetPayload(), m) != nil {
			state.DecodeFailures++
			return
		}
		state.Expectations[m.GetId()] = m

	default:
		state.UnknownPayloads[env.GetPayloadType()]++
	}
}

// Rebuild folds a log from the empty twin. This is the whole recovery
// story of spec §7.2: any store can be dropped and rebuilt from the log.
func Rebuild(log []*aegisv1.Envelope) *Twin {
	state := New()
	for _, env := range log {
		Apply(state, env)
	}
	return state
}

// Proto assembles the canonical aegisv1.Twin projection: every repeated
// field sorted by id. Sorting here — not at insert — keeps the fold O(1)
// per event and makes the canonical order impossible to skip.
func (t *Twin) Proto() *aegisv1.Twin {
	return &aegisv1.Twin{
		Property:           t.Property,
		Posture:            t.Posture,
		Zones:              sortedByID(t.Zones, (*aegisv1.Zone).GetId),
		Devices:            sortedByID(t.Devices, (*aegisv1.Device).GetId),
		Assets:             sortedByID(t.Assets, (*aegisv1.Asset).GetId),
		Entities:           sortedByID(t.Entities, (*aegisv1.Entity).GetId),
		OpenAlerts:         sortedByID(t.OpenAlerts, (*aegisv1.Alert).GetId),
		ActiveExpectations: sortedByID(t.Expectations, (*aegisv1.Expectation).GetId),
	}
}

// MarshalCanonical is the determinism witness: two folds of the same log
// must produce byte-identical output (spec §9.4 acceptance). Sorted slices +
// Deterministic marshal = canonical bytes.
func MarshalCanonical(t *Twin) []byte {
	b, err := proto.MarshalOptions{Deterministic: true}.Marshal(t.Proto())
	if err != nil {
		// Marshalling a well-formed generated message cannot fail short of
		// memory corruption. If it does, the twin's bytes are undefined —
		// fail closed and loudly (CLAUDE.md rule 5), never return a
		// partial canonical form.
		panic("twin: canonical marshal failed: " + err.Error())
	}
	return b
}

// sortedByID turns an id-keyed map into a slice sorted by id. The id
// function reads the message's own id field so a mis-keyed insert cannot
// change the canonical order.
func sortedByID[M any](m map[string]*M, id func(*M) string) []*M {
	if len(m) == 0 {
		return nil
	}
	out := make([]*M, 0, len(m))
	for _, v := range m {
		out = append(out, v)
	}
	sort.Slice(out, func(i, j int) bool { return id(out[i]) < id(out[j]) })
	return out
}
