package lint

import (
	"path/filepath"
	"strings"
	"testing"
)

// testdata is a hermetic repo root: good.yaml is a byte-for-byte copy of
// playbooks/perimeter_breach_night.yaml, and testdata/sim/scenarios/ holds
// stub files at the exact paths its tests: block names, so check 5 resolves
// without touching anything outside this package.
func lintTestdata(t *testing.T, name string) []Violation {
	t.Helper()
	return File(filepath.Join("testdata", name), "testdata")
}

func TestRealPlaybookPasses(t *testing.T) {
	vs := lintTestdata(t, "good.yaml")
	if len(vs) != 0 {
		t.Fatalf("perimeter_breach_night must lint clean, got %d violations: %v", len(vs), vs)
	}
}

func TestSingleViolationFixtures(t *testing.T) {
	cases := []struct {
		file    string
		check   int
		substrs []string
	}{
		{"force_rung.yaml", 1, []string{"spec §3.1", "no rung 7", `"FORCE"`}},
		{"rung_seven.yaml", 1, []string{"spec §3.1", `no rung "7"`}},
		{"announce_on_loop.yaml", 2, []string{"invariant I4", "ANNOUNCE is capped at HUMAN_DIRECTED", "never loosen"}},
		{"unknown_autonomy.yaml", 2, []string{"spec §12.2", `"FULL_AUTO"`}},
		{"bad_max_rung.yaml", 3, []string{"spec §3.1", "roe.max_rung", `"ENGAGE"`, "no rung above"}},
		{"masks_absent.yaml", 4, []string{"spec §3.3", "masks_offsite_optics", "present and true"}},
		{"masks_false.yaml", 4, []string{"spec §3.3", "masks_offsite_optics must be true"}},
		{"empty_tests.yaml", 5, []string{"spec §11.1", "at least one scenario"}},
		{"missing_scenario.yaml", 5, []string{"spec §11.1", "S-999-does-not-exist.yaml", "not found"}},
		{"version_zero.yaml", 6, []string{"spec §11", "positive integer", "got 0"}},
		{"bad_slug.yaml", 6, []string{"spec §11", "[a-z0-9_]+", `"Perimeter-Breach"`}},
		{"supervised_no_abort.yaml", 7, []string{"spec §11.1", "HUMAN_SUPERVISED", "abort_window_seconds"}},
		{"short_abort.yaml", 7, []string{"spec §11.1", "abort_window_seconds 3", "minimum is 5"}},
	}
	for _, tc := range cases {
		t.Run(tc.file, func(t *testing.T) {
			vs := lintTestdata(t, tc.file)
			if len(vs) != 1 {
				t.Fatalf("want exactly 1 violation, got %d: %v", len(vs), vs)
			}
			v := vs[0]
			if v.Check != tc.check {
				t.Errorf("want check %d, got check %d (%s)", tc.check, v.Check, v.Msg)
			}
			for _, sub := range tc.substrs {
				if !strings.Contains(v.Msg, sub) {
					t.Errorf("message must contain %q, got: %s", sub, v.Msg)
				}
			}
		})
	}
}

// The linter reports EVERY violation, never just the first
// (docs/contracts/sim-cli.md §4).
func TestEverythingWrongReportsAllSevenChecks(t *testing.T) {
	vs := lintTestdata(t, "everything_wrong.yaml")
	seen := map[int]bool{}
	for _, v := range vs {
		seen[v.Check] = true
	}
	for check := 1; check <= 7; check++ {
		if !seen[check] {
			t.Errorf("check %d not reported; got: %v", check, vs)
		}
	}
	// name + version are two separate check-6 violations; total must
	// reflect all of them, not a deduplicated view.
	if len(vs) < 8 {
		t.Errorf("want >= 8 violations (every one reported), got %d: %v", len(vs), vs)
	}
}

func TestUnreadableAndUnparseableFiles(t *testing.T) {
	if vs := File(filepath.Join("testdata", "no-such-file.yaml"), "testdata"); len(vs) != 1 || vs[0].Check != 0 {
		t.Errorf("missing file: want one check-0 violation, got %v", vs)
	}
	if vs := Bytes([]byte(":\t not yaml ["), "testdata"); len(vs) != 1 || vs[0].Check != 0 {
		t.Errorf("garbage yaml: want one check-0 violation, got %v", vs)
	}
}

// yaml.v3 silently truncates a float when decoding into an int; the linter
// must still reject version: 1.5 as "not a positive integer".
func TestFloatVersionRejected(t *testing.T) {
	vs := Bytes([]byte("playbook: x\nversion: 1.5\n"), "testdata")
	found := false
	for _, v := range vs {
		if v.Check == 6 && strings.Contains(v.Msg, `"1.5"`) && strings.Contains(v.Msg, "positive integer") {
			found = true
		}
	}
	if !found {
		t.Errorf("want a check-6 violation naming \"1.5\", got %v", vs)
	}
}

// The caps table itself is doctrine: OBSERVE and ILLUMINATE may run up to
// HUMAN_ON_LOOP; ANNOUNCE and above are HUMAN_DIRECTED only; the ladder has
// exactly six rungs (spec §3.1). If this test fails, someone edited a
// compiled-in safety constant — that is a human-review event, not a test to
// update.
func TestCompiledCapsAreDoctrine(t *testing.T) {
	want := map[string]Autonomy{
		"OBSERVE":    HumanOnLoop,
		"ILLUMINATE": HumanOnLoop,
		"ANNOUNCE":   HumanDirected,
		"SHADOW":     HumanDirected,
		"DENY":       HumanDirected,
		"HANDOFF":    HumanDirected,
	}
	if len(compiledCap) != 6 {
		t.Fatalf("the ladder has exactly six rungs, table has %d", len(compiledCap))
	}
	for rung, ceiling := range want {
		if got := compiledCap[rung]; got != ceiling {
			t.Errorf("%s: cap is %s, want %s", rung, got, ceiling)
		}
	}
}
