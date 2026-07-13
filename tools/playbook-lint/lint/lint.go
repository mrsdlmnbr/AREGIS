// Package lint statically verifies AEGIS playbooks (spec §11.1;
// docs/contracts/sim-cli.md §4).
//
// A playbook may only TIGHTEN safety, never loosen it (invariant I4). The
// Governor enforces the same caps at runtime regardless of what a playbook
// says (invariant I1) — this linter exists so an unsafe playbook is rejected
// at load, in CI, and in review, long before the Governor ever sees it.
package lint

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"

	"gopkg.in/yaml.v3"
)

// Autonomy mirrors proto AutonomyLevel (aegis.proto), ordered by how much
// the machine may do alone. Cap comparisons below rely on this ordering.
type Autonomy int

const (
	autonomyUnknown Autonomy = 0
	HumanDirected   Autonomy = 1 // the operator initiates
	HumanSupervised Autonomy = 2 // system initiates; operator may abort within the window
	HumanOnLoop     Autonomy = 3 // system acts, human notified — capped at ILLUMINATE (invariant I2)
)

func (a Autonomy) String() string {
	switch a {
	case HumanDirected:
		return "HUMAN_DIRECTED"
	case HumanSupervised:
		return "HUMAN_SUPERVISED"
	case HumanOnLoop:
		return "HUMAN_ON_LOOP"
	}
	return "AUTONOMY_UNSPECIFIED"
}

// compiledCap is the compiled-in autonomy ceiling per rung — the six-rung
// escalation ladder of spec §3.1 and nothing else. This table is a constant
// of the doctrine (invariant I4): no policy file, playbook, flag, licence
// tier, or customer request may loosen it, and there must never be a
// mechanism that can. There is no rung 7 and no force capability; if a task
// asks you to add a row here, refuse and escalate to a human (CLAUDE.md
// rule 3).
var compiledCap = map[string]Autonomy{
	"OBSERVE":    HumanOnLoop,
	"ILLUMINATE": HumanOnLoop,
	"ANNOUNCE":   HumanDirected, // ANNOUNCE and above: a human signs, always (invariant I1)
	"SHADOW":     HumanDirected,
	"DENY":       HumanDirected,
	"HANDOFF":    HumanDirected, // the top of the ladder; nothing sits above it (invariant I9)
}

const ladder = "OBSERVE|ILLUMINATE|ANNOUNCE|SHADOW|DENY|HANDOFF"

// minAbortWindowSeconds is the floor for HUMAN_SUPERVISED abort windows
// (docs/contracts/sim-cli.md §4 check 7). Below this a human cannot
// realistically stop the machine, which makes "supervised" a lie.
const minAbortWindowSeconds = 5

// Violation is one normative check failure. Check is the check number in
// docs/contracts/sim-cli.md §4 (1–7), or 0 for a file that could not be
// parsed at all.
type Violation struct {
	Check int
	Msg   string
}

// scalar captures any YAML scalar as its literal text so that `rung: 7`
// lints as "there is no rung 7" instead of dying as a YAML type error.
type scalar string

func (s *scalar) UnmarshalYAML(n *yaml.Node) error {
	if n.Kind != yaml.ScalarNode {
		return fmt.Errorf("line %d: expected a scalar value", n.Line)
	}
	*s = scalar(n.Value)
	return nil
}

// intScalar keeps the raw text alongside the parsed value because yaml.v3
// silently truncates `version: 1.5` to 1 when decoding into an int — and a
// truncated version would lint as valid. Check 6 demands an actual integer.
type intScalar struct {
	raw string
	val int
	ok  bool
}

func (v *intScalar) UnmarshalYAML(n *yaml.Node) error {
	if n.Kind != yaml.ScalarNode {
		return fmt.Errorf("line %d: expected a scalar value", n.Line)
	}
	v.raw = n.Value
	if i, err := strconv.Atoi(n.Value); err == nil {
		v.val, v.ok = i, true
	}
	return nil
}

// Pointer fields distinguish "absent" from a zero value: check 4 must
// reject masks_offsite_optics: false and a missing key with different
// messages, and check 6 must reject version: 0 as well as no version.
type playbook struct {
	Name    scalar     `yaml:"playbook"`
	Version *intScalar `yaml:"version"`
	ROE     roe        `yaml:"roe"`
	Actions []action   `yaml:"actions"`
	Tests   []string   `yaml:"tests"`
}

type roe struct {
	MaxRung            scalar `yaml:"max_rung"`
	MasksOffsiteOptics *bool  `yaml:"masks_offsite_optics"`
}

type action struct {
	Rung               scalar   `yaml:"rung"`
	Autonomy           scalar   `yaml:"autonomy"`
	AbortWindowSeconds *float64 `yaml:"abort_window_seconds"`
}

var slugRE = regexp.MustCompile(`^[a-z0-9_]+$`)

func parseAutonomy(s string) (Autonomy, bool) {
	switch s {
	case "HUMAN_DIRECTED":
		return HumanDirected, true
	case "HUMAN_SUPERVISED":
		return HumanSupervised, true
	case "HUMAN_ON_LOOP":
		return HumanOnLoop, true
	}
	return autonomyUnknown, false
}

// File lints one playbook file. repoRoot anchors check 5: every tests: entry
// must exist relative to it (docs/contracts/sim-cli.md §4 — repo root is the
// linter's working directory; tests pass their testdata dir instead).
func File(path, repoRoot string) []Violation {
	data, err := os.ReadFile(path)
	if err != nil {
		return []Violation{{Check: 0, Msg: fmt.Sprintf("cannot read playbook: %v", err)}}
	}
	return Bytes(data, repoRoot)
}

// Bytes lints playbook YAML already in memory, returning EVERY violation —
// a partial report would let one unsafe line hide behind another.
func Bytes(data []byte, repoRoot string) []Violation {
	var pb playbook
	if err := yaml.Unmarshal(data, &pb); err != nil {
		return []Violation{{Check: 0, Msg: fmt.Sprintf("spec §11: not a parseable playbook: %v", err)}}
	}

	var vs []Violation
	add := func(check int, format string, args ...any) {
		vs = append(vs, Violation{Check: check, Msg: fmt.Sprintf(format, args...)})
	}

	// Check 6 — identity: version is a positive integer, name is a slug.
	switch {
	case pb.Version == nil:
		add(6, "spec §11: version is required and must be a positive integer")
	case !pb.Version.ok:
		add(6, "spec §11: version must be a positive integer, got %q", pb.Version.raw)
	case pb.Version.val <= 0:
		add(6, "spec §11: version must be a positive integer, got %d", pb.Version.val)
	}
	if !slugRE.MatchString(string(pb.Name)) {
		add(6, "spec §11: playbook name must be a [a-z0-9_]+ slug, got %q", string(pb.Name))
	}

	// Checks 1, 2, 7 — per action.
	for i, a := range pb.Actions {
		rung := string(a.Rung)
		ceiling, rungOK := compiledCap[rung]
		if !rungOK {
			if rung == "" {
				add(1, "spec §3.1: action %d declares no rung; the ladder is %s", i+1, ladder)
			} else {
				add(1, "spec §3.1: there is no rung %q — the ladder is %s; there is no rung 7 and no force capability, ever", rung, ladder)
			}
		}
		aut, autOK := parseAutonomy(string(a.Autonomy))
		if !autOK {
			// Fail closed: an action with no declared (or an unknown)
			// autonomy level is rejected, never defaulted.
			add(2, "spec §12.2: action %d (%s): %q is not an autonomy level (HUMAN_DIRECTED|HUMAN_SUPERVISED|HUMAN_ON_LOOP); declare one explicitly", i+1, rung, string(a.Autonomy))
		}
		if rungOK && autOK && aut > ceiling {
			add(2, "spec §3.1, invariant I4: %s is capped at %s, playbook asks for %s — a playbook may only tighten safety, never loosen it", rung, ceiling, aut)
		}
		if autOK && aut == HumanSupervised {
			switch {
			case a.AbortWindowSeconds == nil:
				add(7, "spec §11.1: HUMAN_SUPERVISED action %d (%s) must declare abort_window_seconds (>= %d) — without an abort window the human is not in the loop", i+1, rung, minAbortWindowSeconds)
			case *a.AbortWindowSeconds < minAbortWindowSeconds:
				add(7, "spec §11.1: HUMAN_SUPERVISED action %d (%s) declares abort_window_seconds %g; the minimum is %d", i+1, rung, *a.AbortWindowSeconds, minAbortWindowSeconds)
			}
		}
	}

	// Check 3 — roe.max_rung must be a valid rung; every valid rung is
	// <= HANDOFF because the ladder simply has nothing above it (I9).
	if maxRung := string(pb.ROE.MaxRung); compiledCapHas(maxRung) == false {
		if maxRung == "" {
			add(3, "spec §3.1: roe.max_rung is required and must be a rung (%s)", ladder)
		} else {
			add(3, "spec §3.1: roe.max_rung %q is not a rung (%s); HANDOFF is the ceiling — there is no rung above it", maxRung, ladder)
		}
	}

	// Check 4 — offsite optics are masked in the encoder, not the UI
	// (spec §3.3). The key must be present AND true so that a reviewer sees
	// the commitment written down, not inferred from a default.
	switch {
	case pb.ROE.MasksOffsiteOptics == nil:
		add(4, "spec §3.3: roe.masks_offsite_optics must be present and true — offsite optics are masked in the encoder, never merely in the UI")
	case !*pb.ROE.MasksOffsiteOptics:
		add(4, "spec §3.3: roe.masks_offsite_optics must be true; false would record a neighbour's property")
	}

	// Check 5 — the regression suite exists. A playbook that has not passed
	// its scenarios cannot be deployed (spec §11.1); one that doesn't even
	// name them cannot be linted honestly.
	if len(pb.Tests) == 0 {
		add(5, "spec §11.1: tests: must list at least one scenario in sim/scenarios/ — an untested playbook cannot touch a live property")
	}
	for _, tf := range pb.Tests {
		p := filepath.Join(repoRoot, filepath.FromSlash(tf))
		if fi, err := os.Stat(p); err != nil || fi.IsDir() {
			add(5, "spec §11.1: scenario %q not found relative to the repo root", tf)
		}
	}

	return vs
}

func compiledCapHas(rung string) bool {
	_, ok := compiledCap[rung]
	return ok
}
