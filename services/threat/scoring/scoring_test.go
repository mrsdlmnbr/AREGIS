package scoring_test

import (
	"reflect"
	"testing"

	"github.com/mrsdlmnbr/aregis/services/threat/scoring"
)

// s001Input is the golden vector for scenario S-001 (docs/contracts/sim-cli.md
// §1): unknown person, GROUNDS, AWAY, 03h, 3 sensors, pol anomaly 0.8. The
// Rust sim harness embeds the same vector; if this test changes, both goldens
// change in the same commit, with an ADR.
func s001Input() scoring.Input {
	return scoring.Input{
		Schema:           scoring.SchemaV1,
		ObjectClass:      "PERSON",
		IdentityKnown:    false,
		ZoneClass:        "GROUNDS",
		Posture:          "AWAY",
		LocalHour:        3,
		Expected:         false,
		EntityConfidence: 0.9936,
		DistinctSensors:  3,
		Pol: scoring.Pol{
			Available: true,
			Anomaly:   0.8,
			Note:      "last unexpected perimeter entity: 41 days ago",
		},
	}
}

func TestS001Golden(t *testing.T) {
	got, err := scoring.Score(s001Input())
	if err != nil {
		t.Fatalf("Score(S-001) error: %v", err)
	}
	want := scoring.Output{
		Schema:   "aegis.sim.threat/v1",
		Score:    6.6,
		Severity: 4,
		Receipt: []scoring.ReceiptTerm{
			{Name: "base", Input: "PERSON/unknown @ GROUNDS", Weight: 1.0, Contribution: 2.4, Note: ""},
			{Name: "posture_amplifier", Input: "AWAY @ 03h (night)", Weight: 2.0, Contribution: 2.4, Note: ""},
			{Name: "expectation_discount", Input: "no expectation matched", Weight: 0.0, Contribution: 0.0, Note: ""},
			{Name: "pol_anomaly", Input: "last unexpected perimeter entity: 41 days ago", Weight: 1.5, Contribution: 1.2, Note: ""},
			{Name: "corroboration", Input: "3 sensors, 0 mesh", Weight: 1.0, Contribution: 0.6, Note: ""},
			{Name: "dwell", Input: "0.0 s", Weight: 0.5, Contribution: 0.0, Note: ""},
			{Name: "known_benign", Input: "none", Weight: 1.0, Contribution: 0.0, Note: ""},
		},
		PolNote: "last unexpected perimeter entity: 41 days ago",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("S-001 golden mismatch:\n got: %#v\nwant: %#v", got, want)
	}
}

func TestExpectedVisitorClampsAtZero(t *testing.T) {
	got, err := scoring.Score(scoring.Input{
		Schema:          scoring.SchemaV1,
		ObjectClass:     "PERSON",
		IdentityKnown:   true,
		ZoneClass:       "THRESHOLD",
		Posture:         "NOMINAL",
		LocalHour:       9,
		Expected:        true,
		DistinctSensors: 1,
		Pol:             scoring.Pol{Available: true, Anomaly: 0.1, Note: "routine"},
	})
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	if got.Score != 0 {
		t.Errorf("score = %v, want 0 (clamped)", got.Score)
	}
	if got.Severity != 1 {
		t.Errorf("severity = %d, want 1", got.Severity)
	}
	benign := got.Receipt[6]
	if benign.Name != "known_benign" || benign.Contribution != -0.5 || benign.Input != "known+expected" {
		t.Errorf("known_benign term = %#v, want contribution -0.5, input %q", benign, "known+expected")
	}
}

func TestAnimalIsBenign(t *testing.T) {
	got, err := scoring.Score(scoring.Input{
		Schema:          scoring.SchemaV1,
		ObjectClass:     "ANIMAL",
		ZoneClass:       "GROUNDS",
		Posture:         "AWAY",
		LocalHour:       2,
		DistinctSensors: 1,
		Pol:             scoring.Pol{Available: true, Anomaly: 0.1, Note: "routine"},
	})
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	if got.Severity != 1 {
		t.Errorf("severity = %d, want 1", got.Severity)
	}
	benign := got.Receipt[6]
	if benign.Name != "known_benign" || benign.Contribution != -1.0 {
		t.Errorf("known_benign term = %#v, want contribution -1.0", benign)
	}
	if benign.Input != "animal" {
		t.Errorf("known_benign input = %q, want %q", benign.Input, "animal")
	}
}

func TestKnownResidentAtNight(t *testing.T) {
	got, err := scoring.Score(scoring.Input{
		Schema:          scoring.SchemaV1,
		ObjectClass:     "PERSON",
		IdentityKnown:   true,
		ZoneClass:       "THRESHOLD",
		Posture:         "NIGHT",
		LocalHour:       23,
		DistinctSensors: 1,
		Pol:             scoring.Pol{Available: true, Anomaly: 0.1, Note: "routine"},
	})
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	if got.Severity != 1 {
		t.Errorf("severity = %d, want 1 (score %v)", got.Severity, got.Score)
	}
}

func TestAttestationCap(t *testing.T) {
	in := s001Input() // scores severity 4 when attested
	in.AllContributingUnattested = true
	got, err := scoring.Score(in)
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	if got.Severity != 3 {
		t.Errorf("severity = %d, want 3 (capped, spec §9.1)", got.Severity)
	}
	if got.Score != 6.6 {
		t.Errorf("score = %v, want 6.6 (cap changes severity, not score)", got.Score)
	}
	last := got.Receipt[len(got.Receipt)-1]
	if last.Name != "attestation_cap" {
		t.Fatalf("last receipt term = %q, want attestation_cap", last.Name)
	}
	if last.Weight != 1.0 || last.Contribution != 0 {
		t.Errorf("attestation_cap term = %#v, want weight 1.0, contribution 0", last)
	}
	if len(got.Receipt) != 8 {
		t.Errorf("receipt has %d terms, want 8", len(got.Receipt))
	}
}

func TestPolUnavailableContributesZeroAndSaysSo(t *testing.T) {
	in := s001Input()
	in.Pol = scoring.Pol{Available: false, Anomaly: 0.8, Note: "ignored"}
	got, err := scoring.Score(in)
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	pol := got.Receipt[3]
	if pol.Name != "pol_anomaly" {
		t.Fatalf("receipt[3] = %q, want pol_anomaly (order is normative)", pol.Name)
	}
	if pol.Contribution != 0 {
		t.Errorf("pol contribution = %v, want 0", pol.Contribution)
	}
	const want = "pol unavailable — contributing 0"
	if pol.Note != want {
		t.Errorf("pol note = %q, want %q", pol.Note, want)
	}
	if pol.Input != want {
		t.Errorf("pol input = %q, want %q", pol.Input, want)
	}
	if got.PolNote != want {
		t.Errorf("pol_note = %q, want %q", got.PolNote, want)
	}
	// S-001 minus the 1.2 pol term: 5.4, still severity 4.
	if got.Score != 5.4 {
		t.Errorf("score = %v, want 5.4", got.Score)
	}
}

func TestReceiptOrderIsNormative(t *testing.T) {
	got, err := scoring.Score(s001Input())
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	wantOrder := []string{
		"base", "posture_amplifier", "expectation_discount",
		"pol_anomaly", "corroboration", "dwell", "known_benign",
	}
	if len(got.Receipt) != len(wantOrder) {
		t.Fatalf("receipt has %d terms, want %d", len(got.Receipt), len(wantOrder))
	}
	for i, name := range wantOrder {
		if got.Receipt[i].Name != name {
			t.Errorf("receipt[%d] = %q, want %q", i, got.Receipt[i].Name, name)
		}
	}
}

func TestUnknownEnumsFailClosed(t *testing.T) {
	cases := map[string]func(*scoring.Input){
		"object_class": func(in *scoring.Input) { in.ObjectClass = "GHOST" },
		"zone_class":   func(in *scoring.Input) { in.ZoneClass = "MOAT" },
		"posture":      func(in *scoring.Input) { in.Posture = "PANIC" },
		"schema":       func(in *scoring.Input) { in.Schema = "aegis.sim.threat/v2" },
		"local_hour":   func(in *scoring.Input) { in.LocalHour = 24 },
	}
	for name, mutate := range cases {
		t.Run(name, func(t *testing.T) {
			in := s001Input()
			mutate(&in)
			if _, err := scoring.Score(in); err == nil {
				t.Errorf("Score accepted bad %s; must fail closed", name)
			}
		})
	}
}

// TestCorroborationAndDwellSaturate pins the term caps: sensors cap at 0.9,
// mesh at 2 corroborations, dwell at 60 s.
func TestCorroborationAndDwellSaturate(t *testing.T) {
	in := s001Input()
	in.DistinctSensors = 10
	in.MeshCorroborations = 5
	in.DwellSeconds = 600
	got, err := scoring.Score(in)
	if err != nil {
		t.Fatalf("Score error: %v", err)
	}
	if corr := got.Receipt[4]; corr.Contribution != 1.7 {
		t.Errorf("corroboration = %v, want 1.7 (0.9 sensor cap + 0.8 mesh cap)", corr.Contribution)
	}
	if dwell := got.Receipt[5]; dwell.Contribution != 0.5 {
		t.Errorf("dwell = %v, want 0.5 (60 s cap)", dwell.Contribution)
	}
	if got.Receipt[5].Input != "600.0 s" {
		t.Errorf("dwell input = %q, want %q", got.Receipt[5].Input, "600.0 s")
	}
}
