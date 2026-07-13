package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mrsdlmnbr/aregis/services/evidence/seal"
)

// Test-only seed (RFC 8032 vector); production keys live in the HSM (§19).
const testSeedHex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"

func requestJSON(t *testing.T, mutate func(*seal.Request)) []byte {
	t.Helper()
	req := seal.Request{
		Schema:         seal.Schema,
		IncidentID:     "S-001/alert-1",
		PropertyID:     "ridgeline",
		SealedAt:       "2026-03-14T03:11:45.500Z",
		SealedBy:       "appliance:ridgeline",
		EventIDs:       []string{"evt-aaa"},
		AuditRecordIDs: []string{"aud-bbb"},
		Media: []seal.RequestMedia{
			{Name: "frame-sig-1.bin", B64: base64.StdEncoding.EncodeToString([]byte("frame one bytes"))},
		},
		SigningKeySeedHex: testSeedHex,
	}
	if mutate != nil {
		mutate(&req)
	}
	raw, err := json.Marshal(req)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func TestRunSealHappyPath(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "bundle")
	var stdout, stderr bytes.Buffer
	code := run([]string{"seal", "--out", dir}, bytes.NewReader(requestJSON(t, nil)), &stdout, &stderr)
	if code != 0 {
		t.Fatalf("exit %d, stderr: %s", code, stderr.String())
	}
	onDisk, err := os.ReadFile(filepath.Join(dir, "manifest.json"))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(stdout.Bytes(), onDisk) {
		t.Fatal("stdout is not the exact bytes of manifest.json")
	}
}

// The CLI must reject a request without a signing key seed — it must never
// invent a key (docs/contracts/sim-cli.md §2, spec §19).
func TestRunRejectsMissingSigningKeySeed(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "bundle")
	var stdout, stderr bytes.Buffer
	code := run([]string{"seal", "--out", dir},
		bytes.NewReader(requestJSON(t, func(r *seal.Request) { r.SigningKeySeedHex = "" })),
		&stdout, &stderr)
	if code == 0 {
		t.Fatal("expected non-zero exit for request without signing key seed")
	}
	if stdout.Len() != 0 {
		t.Fatal("no manifest may be emitted on failure")
	}
	if !strings.Contains(stderr.String(), "refusing to invent") {
		t.Fatalf("stderr should say the key will not be invented, got: %s", stderr.String())
	}
}

func TestRunRejectsWrongSchema(t *testing.T) {
	var stdout, stderr bytes.Buffer
	code := run([]string{"seal", "--out", filepath.Join(t.TempDir(), "b")},
		bytes.NewReader(requestJSON(t, func(r *seal.Request) { r.Schema = "something/else" })),
		&stdout, &stderr)
	if code == 0 {
		t.Fatal("expected non-zero exit for wrong schema")
	}
}

func TestRunRequiresOutFlag(t *testing.T) {
	var stdout, stderr bytes.Buffer
	if code := run([]string{"seal"}, bytes.NewReader(requestJSON(t, nil)), &stdout, &stderr); code != 2 {
		t.Fatalf("want exit 2 without --out, got %d", code)
	}
}

func TestRunRejectsUnknownSubcommand(t *testing.T) {
	var stdout, stderr bytes.Buffer
	if code := run([]string{"unseal"}, bytes.NewReader(nil), &stdout, &stderr); code != 2 {
		t.Fatalf("want exit 2 for unknown subcommand, got %d", code)
	}
}

func TestRunRejectsBadJSON(t *testing.T) {
	var stdout, stderr bytes.Buffer
	code := run([]string{"seal", "--out", filepath.Join(t.TempDir(), "b")},
		strings.NewReader("{not json"), &stdout, &stderr)
	if code == 0 {
		t.Fatal("expected non-zero exit for invalid JSON")
	}
}

// Same stdin, byte-identical stdout — the determinism rule that binds every
// sim CLI (docs/contracts/sim-cli.md).
func TestRunDeterministicStdout(t *testing.T) {
	in := requestJSON(t, nil)
	var out1, out2, stderr bytes.Buffer
	if code := run([]string{"seal", "--out", filepath.Join(t.TempDir(), "b1")}, bytes.NewReader(in), &out1, &stderr); code != 0 {
		t.Fatalf("first run failed: %s", stderr.String())
	}
	if code := run([]string{"seal", "--out", filepath.Join(t.TempDir(), "b2")}, bytes.NewReader(in), &out2, &stderr); code != 0 {
		t.Fatalf("second run failed: %s", stderr.String())
	}
	if !bytes.Equal(out1.Bytes(), out2.Bytes()) {
		t.Fatal("same stdin produced different stdout")
	}
}
