package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func write(t *testing.T, f Fixture) string {
	t.Helper()
	data, _ := json.Marshal(f)
	p := filepath.Join(t.TempDir(), "fixture.json")
	if err := os.WriteFile(p, data, 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestEmitsStampedEnvelopesDeterministically(t *testing.T) {
	p := write(t, Fixture{
		DeviceID:     "CAM-04",
		Authority:    "OWNED",
		AuthorityRef: "estate-owner-ridgeline",
		Attested:     true,
		Frames: []Frame{
			{CapturedAt: "2026-03-14T03:11:43.600Z", PayloadB64: "ZnJhbWUx"},
			{CapturedAt: "2026-03-14T03:11:44.400Z", PayloadB64: "ZnJhbWUy"},
		},
	})
	var a, b bytes.Buffer
	if err := run(p, json.NewEncoder(&a)); err != nil {
		t.Fatal(err)
	}
	if err := run(p, json.NewEncoder(&b)); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(a.Bytes(), b.Bytes()) {
		t.Fatal("driver output must be byte-identical across runs (A7)")
	}
	if got := strings.Count(a.String(), "\n"); got != 2 {
		t.Fatalf("want 2 envelope lines, got %d", got)
	}
	if !strings.Contains(a.String(), `"authority_ref":"estate-owner-ridgeline"`) {
		t.Fatal("provenance must be stamped at source")
	}
}

func TestNoAuthorityRefNoEmission(t *testing.T) {
	p := write(t, Fixture{
		DeviceID:  "SNEAKY-CAM",
		Authority: "PUBLIC_OPEN",
		// technically accessible; not authorized (spec §3.4)
		Frames: []Frame{{CapturedAt: "2026-03-14T03:11:43Z", PayloadB64: "eA=="}},
	})
	var out bytes.Buffer
	err := run(p, json.NewEncoder(&out))
	if err == nil {
		t.Fatal("a fixture without authority_ref must be refused")
	}
	if out.Len() != 0 {
		t.Fatal("nothing may be emitted before the refusal")
	}
}
