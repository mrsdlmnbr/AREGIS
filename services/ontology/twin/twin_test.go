package twin

import (
	"bytes"
	"math/rand"
	"sort"
	"testing"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/ontology/internal/synthlog"
	"google.golang.org/protobuf/proto"
)

// TestRebuildDeterminism is the §9.4 acceptance in miniature: two
// independent folds of the same log are byte-identical.
func TestRebuildDeterminism(t *testing.T) {
	log := synthlog.Log(7, 200)
	a := MarshalCanonical(Rebuild(log))
	b := MarshalCanonical(Rebuild(log))
	if !bytes.Equal(a, b) {
		t.Fatalf("two folds of the same log differ: %d vs %d bytes", len(a), len(b))
	}
	if len(a) == 0 {
		t.Fatal("canonical twin is empty — the fold applied nothing")
	}
}

// TestRebuildShuffledResortedByOffset shuffles the log, re-sorts it by its
// original offset (restoring the per-key order the event log guarantees,
// spec §7.1), and folds twice. Byte-identical output proves the fold has no
// hidden dependence on slice identity, map iteration order, or anything
// other than the ordered log itself.
func TestRebuildShuffledResortedByOffset(t *testing.T) {
	log := synthlog.Log(7, 200)
	want := MarshalCanonical(Rebuild(log))

	type offsetEnv struct {
		offset int
		env    *aegisv1.Envelope
	}
	shuffled := make([]offsetEnv, len(log))
	for i, env := range log {
		shuffled[i] = offsetEnv{offset: i, env: env}
	}
	r := rand.New(rand.NewSource(99))
	r.Shuffle(len(shuffled), func(i, j int) { shuffled[i], shuffled[j] = shuffled[j], shuffled[i] })
	sort.Slice(shuffled, func(i, j int) bool { return shuffled[i].offset < shuffled[j].offset })

	resorted := make([]*aegisv1.Envelope, len(shuffled))
	for i, oe := range shuffled {
		resorted[i] = oe.env
	}

	got1 := MarshalCanonical(Rebuild(resorted))
	got2 := MarshalCanonical(Rebuild(resorted))
	if !bytes.Equal(got1, got2) {
		t.Fatal("two folds of the resorted log differ")
	}
	if !bytes.Equal(want, got1) {
		t.Fatal("fold of shuffled-then-resorted log differs from fold of original log")
	}
}

// TestUnknownPayloadIsCountedNoOp: logs outlive code. A payload type this
// fold does not know must change nothing in the twin and must be counted —
// never an error, never silent.
func TestUnknownPayloadIsCountedNoOp(t *testing.T) {
	state := New()
	Apply(state, mustEnvelope(t, "aegis.v1.Entity", &aegisv1.Entity{Id: "ent-01"}))
	before := MarshalCanonical(state)

	Apply(state, &aegisv1.Envelope{
		EventId:     "evt-unknown",
		PayloadType: "aegis.v1.SomeFutureKind",
		Payload:     []byte{0xde, 0xad},
	})

	after := MarshalCanonical(state)
	if !bytes.Equal(before, after) {
		t.Fatal("unknown payload type mutated the twin; must be a no-op")
	}
	if got := state.UnknownPayloads["aegis.v1.SomeFutureKind"]; got != 1 {
		t.Fatalf("unknown payload counter = %d, want 1", got)
	}
}

// TestUndecodablePayloadIsCountedSkip: a known type with corrupt bytes is
// counted and skipped — the fold stays total.
func TestUndecodablePayloadIsCountedSkip(t *testing.T) {
	state := New()
	Apply(state, &aegisv1.Envelope{
		EventId:     "evt-corrupt",
		PayloadType: PayloadEntity,
		Payload:     []byte{0xff, 0xff, 0xff, 0xff},
	})
	if state.DecodeFailures != 1 {
		t.Fatalf("DecodeFailures = %d, want 1", state.DecodeFailures)
	}
	if len(state.Entities) != 0 {
		t.Fatal("corrupt payload produced an entity")
	}
}

// TestApplyPayloadKinds exercises each folded kind once.
func TestApplyPayloadKinds(t *testing.T) {
	state := New()

	Apply(state, mustEnvelope(t, PayloadEntity, &aegisv1.Entity{Id: "ent-01", DwellSeconds: 12}))
	Apply(state, mustEnvelope(t, PayloadDevice, &aegisv1.Device{Id: "dev-01", State: aegisv1.DeviceState_DEGRADED}))
	Apply(state, mustEnvelope(t, PayloadExpectation, &aegisv1.Expectation{Id: "exp-01", PersonId: "person-01"}))
	Apply(state, mustEnvelope(t, PayloadPosture, &aegisv1.SetPostureRequest{Posture: aegisv1.Posture_AWAY}))
	Apply(state, mustEnvelope(t, PayloadAlert, &aegisv1.Alert{Id: "alert-01", State: aegisv1.AlertState_OPEN}))

	if state.Entities["ent-01"] == nil || state.Entities["ent-01"].GetDwellSeconds() != 12 {
		t.Fatal("entity upsert not applied")
	}
	if state.Devices["dev-01"].GetState() != aegisv1.DeviceState_DEGRADED {
		t.Fatal("device state change not applied")
	}
	if state.Expectations["exp-01"].GetPersonId() != "person-01" {
		t.Fatal("expectation upsert not applied")
	}
	if state.Posture != aegisv1.Posture_AWAY {
		t.Fatal("posture change not applied")
	}
	if state.OpenAlerts["alert-01"] == nil {
		t.Fatal("alert open not applied")
	}

	// Last-write-wins upsert.
	Apply(state, mustEnvelope(t, PayloadEntity, &aegisv1.Entity{Id: "ent-01", DwellSeconds: 99}))
	if state.Entities["ent-01"].GetDwellSeconds() != 99 {
		t.Fatal("entity upsert is not last-write-wins")
	}

	// A dismissal leaves the open-alert projection (the log keeps the record).
	Apply(state, mustEnvelope(t, PayloadAlert, &aegisv1.Alert{Id: "alert-01", State: aegisv1.AlertState_DISMISSED}))
	if state.OpenAlerts["alert-01"] != nil {
		t.Fatal("dismissed alert still projected as open")
	}
}

// TestMarshalCanonicalSorted: the canonical projection sorts every repeated
// field by id regardless of insertion order.
func TestMarshalCanonicalSorted(t *testing.T) {
	state := New()
	for _, id := range []string{"ent-09", "ent-01", "ent-05"} {
		Apply(state, mustEnvelope(t, PayloadEntity, &aegisv1.Entity{Id: id}))
	}
	p := state.Proto()
	if len(p.GetEntities()) != 3 {
		t.Fatalf("entities = %d, want 3", len(p.GetEntities()))
	}
	if !sort.SliceIsSorted(p.GetEntities(), func(i, j int) bool {
		return p.GetEntities()[i].GetId() < p.GetEntities()[j].GetId()
	}) {
		t.Fatal("canonical entities not sorted by id")
	}
}

func mustEnvelope(t *testing.T, payloadType string, msg proto.Message) *aegisv1.Envelope {
	t.Helper()
	b, err := proto.MarshalOptions{Deterministic: true}.Marshal(msg)
	if err != nil {
		t.Fatalf("marshal %s: %v", payloadType, err)
	}
	return &aegisv1.Envelope{
		EventId:     "evt-test",
		PropertyId:  "ridgeline",
		PayloadType: payloadType,
		Payload:     b,
	}
}
