// Package scoring is the deterministic, explainable severity scorer of
// spec §9.6, exposed to the sim harness via docs/contracts/sim-cli.md §1.
//
// HARD RULE (spec §9.6, CLAUDE.md rule 2): no neural network sets severity.
// Every constant in this file is compiled in, not config. The number that
// decides whether a family is woken at 03:00 must be defensible line by
// line, so every term is emitted into the receipt with its input, weight and
// contribution — including zero contributions, so the operator sees that a
// discount did *not* apply and why.
//
// Determinism (contract rule): same Input → identical Output, forever. Time
// is an input (axiom A7): local_hour arrives in the request; nothing here
// reads a clock.
//
// Fail closed (CLAUDE.md rule 5): unknown enum strings are errors, never
// defaults. A typo'd posture must not silently score as NOMINAL.
package scoring

import (
	"fmt"
	"math"
	"strings"
)

// SchemaV1 is the only schema this scorer accepts.
const SchemaV1 = "aegis.sim.threat/v1"

// polUnavailableNote is normative wording (contract §1, spec §9.5 failure
// mode): pol down ⇒ contributes 0 and the receipt says so. Never silently.
const polUnavailableNote = "pol unavailable — contributing 0"

// Pol is the pattern-of-life anomaly input (spec §9.5).
type Pol struct {
	Available bool    `json:"available"`
	Anomaly   float64 `json:"anomaly"`
	Note      string  `json:"note"`
}

// Input mirrors the stdin object of contract §1.
type Input struct {
	Schema                    string  `json:"schema"`
	ObjectClass               string  `json:"object_class"`
	IdentityKnown             bool    `json:"identity_known"`
	ZoneClass                 string  `json:"zone_class"`
	Posture                   string  `json:"posture"`
	LocalHour                 int     `json:"local_hour"`
	Expected                  bool    `json:"expected"`
	EntityConfidence          float64 `json:"entity_confidence"`
	DistinctSensors           int     `json:"distinct_sensors"`
	MeshCorroborations        int     `json:"mesh_corroborations"`
	DwellSeconds              float64 `json:"dwell_seconds"`
	Pol                       Pol     `json:"pol"`
	AllContributingUnattested bool    `json:"all_contributing_unattested"`
}

// ReceiptTerm is one line of the severity receipt. The receipt is the
// feature (spec §9.6): the console renders exactly these fields.
type ReceiptTerm struct {
	Name         string  `json:"name"`
	Input        string  `json:"input"`
	Weight       float64 `json:"weight"`
	Contribution float64 `json:"contribution"`
	Note         string  `json:"note"`
}

// Output mirrors the stdout object of contract §1. Field order is the
// encoding order.
type Output struct {
	Schema   string        `json:"schema"`
	Score    float64       `json:"score"`
	Severity int           `json:"severity"`
	Receipt  []ReceiptTerm `json:"receipt"`
	PolNote  string        `json:"pol_note"`
}

// baseTable is the normative base table of contract §1 (identity unknown).
var baseTable = map[string]map[string]float64{
	"PERSON":      {"PERIMETER": 2.0, "GROUNDS": 2.4, "THRESHOLD": 3.0, "INTERIOR": 4.0, "PRIVATE": 4.5, "SAFE_ROOM": 5.0},
	"VEHICLE":     {"PERIMETER": 1.6, "GROUNDS": 2.0, "THRESHOLD": 2.6, "INTERIOR": 3.6, "PRIVATE": 4.0, "SAFE_ROOM": 4.5},
	"DRONE":       {"PERIMETER": 2.2, "GROUNDS": 2.6, "THRESHOLD": 3.0, "INTERIOR": 3.6, "PRIVATE": 4.0, "SAFE_ROOM": 4.5},
	"ANIMAL":      {"PERIMETER": 0.2, "GROUNDS": 0.2, "THRESHOLD": 0.4, "INTERIOR": 0.8, "PRIVATE": 1.0, "SAFE_ROOM": 1.0},
	"PACKAGE":     {"PERIMETER": 0.4, "GROUNDS": 0.4, "THRESHOLD": 0.8, "INTERIOR": 1.0, "PRIVATE": 1.2, "SAFE_ROOM": 1.2},
	"UNKNOWN_OBJ": {"PERIMETER": 1.0, "GROUNDS": 1.2, "THRESHOLD": 1.6, "INTERIOR": 2.2, "PRIVATE": 2.6, "SAFE_ROOM": 3.0},
}

// postureFactor is the normative posture factor table of contract §1.
var postureFactor = map[string]float64{
	"NOMINAL":  1.0,
	"NIGHT":    1.4,
	"AWAY":     1.6,
	"ELEVATED": 1.8,
	"LOCKDOWN": 2.2,
}

// Score computes severity 1–5 with a receipt, byte-exactly per contract §1.
// It is a pure function: no clock, no I/O, no randomness.
func Score(in Input) (Output, error) {
	if in.Schema != SchemaV1 {
		return Output{}, fmt.Errorf("schema must be %q, got %q", SchemaV1, in.Schema)
	}
	row, ok := baseTable[in.ObjectClass]
	if !ok {
		return Output{}, fmt.Errorf("unknown object_class %q", in.ObjectClass)
	}
	base, ok := row[in.ZoneClass]
	if !ok {
		return Output{}, fmt.Errorf("unknown zone_class %q", in.ZoneClass)
	}
	factor, ok := postureFactor[in.Posture]
	if !ok {
		return Output{}, fmt.Errorf("unknown posture %q", in.Posture)
	}
	if in.LocalHour < 0 || in.LocalHour > 23 {
		return Output{}, fmt.Errorf("local_hour must be 0–23, got %d", in.LocalHour)
	}
	if in.DistinctSensors < 0 {
		return Output{}, fmt.Errorf("distinct_sensors must be ≥ 0, got %d", in.DistinctSensors)
	}
	if in.MeshCorroborations < 0 {
		return Output{}, fmt.Errorf("mesh_corroborations must be ≥ 0, got %d", in.MeshCorroborations)
	}
	if in.DwellSeconds < 0 {
		return Output{}, fmt.Errorf("dwell_seconds must be ≥ 0, got %v", in.DwellSeconds)
	}

	identity := "unknown"
	if in.IdentityKnown {
		base *= 0.25
		identity = "known"
	}

	night := in.LocalHour >= 22 || in.LocalHour < 6
	amp := factor
	nightSuffix := ""
	if night {
		amp = factor * 1.25
		nightSuffix = " (night)"
	}

	disc := 0.0
	expectationInput := "no expectation matched"
	if in.Expected {
		disc = 0.9
		expectationInput = "expectation matched"
	}

	polTerm := 0.0
	polInput := polUnavailableNote
	polTermNote := polUnavailableNote
	polNote := polUnavailableNote
	if in.Pol.Available {
		polTerm = in.Pol.Anomaly * 1.5
		polInput = in.Pol.Note
		polTermNote = ""
		polNote = in.Pol.Note
	}

	corrTerm := math.Min(0.3*float64(max(in.DistinctSensors-1, 0)), 0.9) +
		0.4*float64(min(in.MeshCorroborations, 2))

	dwellTerm := math.Min(in.DwellSeconds/60.0, 1.0) * 0.5

	benign := 0.0
	var benignParts []string
	if in.ObjectClass == "ANIMAL" {
		benign += 1.0
		benignParts = append(benignParts, "animal")
	}
	if in.IdentityKnown && in.Expected {
		benign += 0.5
		benignParts = append(benignParts, "known+expected")
	}
	benignInput := "none"
	if len(benignParts) > 0 {
		benignInput = strings.Join(benignParts, ", ")
	}

	// "running" multiplicative accumulator, exactly as contract §1 defines
	// the receipt: r0 after base, r1 after posture, r2 after expectation.
	r0 := base
	r1 := r0 * amp
	r2 := r1 * (1.0 - disc)

	score := r2 + polTerm + corrTerm + dwellTerm - benign
	if score < 0 {
		score = 0
	}

	severity := bucket(score)

	receipt := []ReceiptTerm{
		{Name: "base", Input: fmt.Sprintf("%s/%s @ %s", in.ObjectClass, identity, in.ZoneClass), Weight: 1.0, Contribution: round6(r0)},
		{Name: "posture_amplifier", Input: fmt.Sprintf("%s @ %02dh%s", in.Posture, in.LocalHour, nightSuffix), Weight: amp, Contribution: round6(r1 - r0)},
		{Name: "expectation_discount", Input: expectationInput, Weight: disc, Contribution: round6(-(r1 * disc))},
		{Name: "pol_anomaly", Input: polInput, Weight: 1.5, Contribution: round6(polTerm), Note: polTermNote},
		{Name: "corroboration", Input: fmt.Sprintf("%d sensors, %d mesh", in.DistinctSensors, in.MeshCorroborations), Weight: 1.0, Contribution: round6(corrTerm)},
		{Name: "dwell", Input: fmt.Sprintf("%.1f s", in.DwellSeconds), Weight: 0.5, Contribution: round6(dwellTerm)},
		{Name: "known_benign", Input: benignInput, Weight: 1.0, Contribution: round6(-benign)},
	}

	// Attestation rule (spec §9.1): unattested events cannot alone raise
	// severity above 3. The cap term is appended whenever the rule was in
	// force, so the operator sees it even when it changed nothing.
	if in.AllContributingUnattested {
		severity = min(severity, 3)
		receipt = append(receipt, ReceiptTerm{
			Name:         "attestation_cap",
			Input:        "all contributing events unattested",
			Weight:       1.0,
			Contribution: 0,
			Note:         "severity capped at 3",
		})
	}

	return Output{
		Schema:   SchemaV1,
		Score:    round6(score),
		Severity: severity,
		Receipt:  receipt,
		PolNote:  polNote,
	}, nil
}

// bucket maps score → severity per the normative default thresholds of
// contract §1. Per-property versioned configs arrive in M1+; M0 compiles in
// the defaults.
func bucket(score float64) int {
	switch {
	case score < 1.5:
		return 1
	case score < 3.0:
		return 2
	case score < 4.5:
		return 3
	case score < 7.5:
		return 4
	default:
		return 5
	}
}

// round6 rounds to 6 decimal places (half away from zero) before encoding,
// per contract §1, and normalises −0 so JSON never carries "-0".
func round6(v float64) float64 {
	r := math.Round(v*1e6) / 1e6
	if r == 0 {
		return 0
	}
	return r
}
