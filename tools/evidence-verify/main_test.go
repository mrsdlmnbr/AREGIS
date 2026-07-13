package main

// These tests seal real bundles with services/evidence/seal and then verify
// them with this package's independent re-implementation — two codebases
// written from the same contract must agree, and every tamper must be
// caught and named. The seal import is TEST-ONLY: it is not compiled into
// the evidence-verify binary, which stays pure stdlib (see main.go).

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/mrsdlmnbr/aregis/services/evidence/seal"
)

// Test-only seed (RFC 8032 vector); production keys live in the HSM (§19).
const testSeedHex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"

const (
	mediaOneName  = "frame-sig-1.bin"
	mediaOneBytes = "frame one bytes"
	mediaTwoName  = "clip-2.bin"
	mediaTwoBytes = "clip two bytes"
)

func sealBundle(t *testing.T, mutate func(*seal.Request)) string {
	t.Helper()
	req := seal.Request{
		Schema:         seal.Schema,
		IncidentID:     "S-001/alert-1",
		PropertyID:     "ridgeline",
		SealedAt:       "2026-03-14T03:11:45.500Z",
		SealedBy:       "appliance:ridgeline",
		EventIDs:       []string{"evt-aaa", "evt-bbb"},
		AuditRecordIDs: []string{"aud-ccc", "aud-ddd"},
		Media: []seal.RequestMedia{
			{Name: mediaOneName, B64: base64.StdEncoding.EncodeToString([]byte(mediaOneBytes))},
			{Name: mediaTwoName, B64: base64.StdEncoding.EncodeToString([]byte(mediaTwoBytes))},
		},
		SigningKeySeedHex: testSeedHex,
	}
	if mutate != nil {
		mutate(&req)
	}
	dir := t.TempDir()
	if _, err := seal.Seal(req, dir); err != nil {
		t.Fatalf("sealing test bundle: %v", err)
	}
	return dir
}

func mediaPath(t *testing.T, dir, content string) string {
	t.Helper()
	sum := sha256.Sum256([]byte(content))
	return filepath.Join(dir, "media", hex.EncodeToString(sum[:]))
}

func flipByte(t *testing.T, path string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	data[0] ^= 0xff
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func editManifest(t *testing.T, dir string, edit func(m map[string]any)) {
	t.Helper()
	p := filepath.Join(dir, "manifest.json")
	raw, err := os.ReadFile(p)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatal(err)
	}
	edit(m)
	out, err := json.Marshal(m)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, out, 0o644); err != nil {
		t.Fatal(err)
	}
}

// flipHexDigit returns s with its first digit replaced by a different valid
// hex digit, so the value still decodes but no longer matches.
func flipHexDigit(s string) string {
	b := []byte(s)
	if b[0] == 'a' {
		b[0] = 'b'
	} else {
		b[0] = 'a'
	}
	return string(b)
}

func contains(list []string, s string) bool {
	for _, v := range list {
		if v == s {
			return true
		}
	}
	return false
}

func TestRoundtripSealThenVerify(t *testing.T) {
	dir := sealBundle(t, nil)
	res := Verify(dir)
	if !res.Valid || len(res.FailedArtifacts) != 0 {
		t.Fatalf("fresh bundle must verify, got %+v", res)
	}
	// Contract shape on stdout: failed_artifacts is [], never null.
	out, err := json.Marshal(res)
	if err != nil {
		t.Fatal(err)
	}
	if string(out) != `{"valid":true,"failed_artifacts":[]}` {
		t.Fatalf("wire shape drifted: %s", out)
	}
}

// Odd total leaf counts exercise unpaired-node promotion through both the
// sealer and this verifier's independent fold.
func TestRoundtripOddLeafCounts(t *testing.T) {
	cases := map[string]func(*seal.Request){
		"3 leaves (1 media, 1 event, 1 audit)": func(r *seal.Request) {
			r.Media = r.Media[:1]
			r.EventIDs = r.EventIDs[:1]
			r.AuditRecordIDs = r.AuditRecordIDs[:1]
		},
		"5 leaves (2 media, 2 events, 1 audit)": func(r *seal.Request) {
			r.AuditRecordIDs = r.AuditRecordIDs[:1]
		},
		"1 leaf (single event, no media)": func(r *seal.Request) {
			r.Media = nil
			r.EventIDs = r.EventIDs[:1]
			r.AuditRecordIDs = nil
		},
	}
	for name, mutate := range cases {
		t.Run(name, func(t *testing.T) {
			res := Verify(sealBundle(t, mutate))
			if !res.Valid {
				t.Fatalf("bundle must verify, failed: %v", res.FailedArtifacts)
			}
		})
	}
}

// Spec §9.9 acceptance: flip any single byte of any artifact → verify fails
// and names the artifact. Exactly the artifact — nothing else may be
// implicated, or the operator cannot trust the naming.
func TestTamperMediaByteNamesExactlyThatArtifact(t *testing.T) {
	dir := sealBundle(t, nil)
	flipByte(t, mediaPath(t, dir, mediaOneBytes))
	res := Verify(dir)
	if res.Valid {
		t.Fatal("tampered bundle verified")
	}
	if want := []string{mediaOneName}; !reflect.DeepEqual(res.FailedArtifacts, want) {
		t.Fatalf("failed_artifacts = %v, want exactly %v", res.FailedArtifacts, want)
	}
}

func TestRemovedMediaFileFails(t *testing.T) {
	dir := sealBundle(t, nil)
	if err := os.Remove(mediaPath(t, dir, mediaTwoBytes)); err != nil {
		t.Fatal(err)
	}
	res := Verify(dir)
	if res.Valid {
		t.Fatal("bundle with missing media verified")
	}
	if want := []string{mediaTwoName}; !reflect.DeepEqual(res.FailedArtifacts, want) {
		t.Fatalf("failed_artifacts = %v, want exactly %v", res.FailedArtifacts, want)
	}
}

func TestTamperedMerkleRootNamed(t *testing.T) {
	dir := sealBundle(t, nil)
	editManifest(t, dir, func(m map[string]any) {
		m["merkle_root"] = flipHexDigit(m["merkle_root"].(string))
	})
	res := Verify(dir)
	if res.Valid {
		t.Fatal("bundle with tampered merkle_root verified")
	}
	if !contains(res.FailedArtifacts, "merkle_root") {
		t.Fatalf("failed_artifacts = %v, must name merkle_root", res.FailedArtifacts)
	}
}

func TestTamperedSignatureNamed(t *testing.T) {
	dir := sealBundle(t, nil)
	editManifest(t, dir, func(m map[string]any) {
		m["signature"] = flipHexDigit(m["signature"].(string))
	})
	res := Verify(dir)
	if res.Valid {
		t.Fatal("bundle with tampered signature verified")
	}
	if want := []string{"signature"}; !reflect.DeepEqual(res.FailedArtifacts, want) {
		t.Fatalf("failed_artifacts = %v, want exactly %v", res.FailedArtifacts, want)
	}
}

// An event id is a Merkle leaf: rewriting one in the manifest must break the
// rebuilt root against the signed one.
func TestTamperedEventIDFails(t *testing.T) {
	dir := sealBundle(t, nil)
	editManifest(t, dir, func(m map[string]any) {
		ids := m["event_ids"].([]any)
		ids[0] = "evt-FORGED"
	})
	res := Verify(dir)
	if res.Valid {
		t.Fatal("bundle with tampered event id verified")
	}
	if !contains(res.FailedArtifacts, "merkle_root") {
		t.Fatalf("failed_artifacts = %v, expected merkle_root mismatch", res.FailedArtifacts)
	}
}

func TestTamperedAuditRecordIDFails(t *testing.T) {
	dir := sealBundle(t, nil)
	editManifest(t, dir, func(m map[string]any) {
		ids := m["audit_record_ids"].([]any)
		ids[1] = "aud-FORGED"
	})
	if res := Verify(dir); res.Valid {
		t.Fatal("bundle with tampered audit record id verified")
	}
}

// Every failure is collected — a tampered artifact must not hide behind an
// earlier one.
func TestCollectsAllFailures(t *testing.T) {
	dir := sealBundle(t, nil)
	flipByte(t, mediaPath(t, dir, mediaOneBytes))
	flipByte(t, mediaPath(t, dir, mediaTwoBytes))
	editManifest(t, dir, func(m map[string]any) {
		m["signature"] = flipHexDigit(m["signature"].(string))
	})
	res := Verify(dir)
	if res.Valid {
		t.Fatal("multiply-tampered bundle verified")
	}
	for _, want := range []string{mediaOneName, mediaTwoName, "signature"} {
		if !contains(res.FailedArtifacts, want) {
			t.Errorf("failed_artifacts = %v, missing %q", res.FailedArtifacts, want)
		}
	}
	if len(res.FailedArtifacts) != 3 {
		t.Errorf("failed_artifacts = %v, want exactly 3 entries", res.FailedArtifacts)
	}
}

func TestMissingManifestFails(t *testing.T) {
	res := Verify(t.TempDir())
	if res.Valid {
		t.Fatal("empty dir verified")
	}
	if want := []string{"manifest.json"}; !reflect.DeepEqual(res.FailedArtifacts, want) {
		t.Fatalf("failed_artifacts = %v, want %v", res.FailedArtifacts, want)
	}
}

func TestWrongSchemaFails(t *testing.T) {
	dir := sealBundle(t, nil)
	editManifest(t, dir, func(m map[string]any) {
		m["schema"] = "aegis.sim.evidence/v999"
	})
	res := Verify(dir)
	if res.Valid {
		t.Fatal("bundle with wrong schema verified")
	}
	if !contains(res.FailedArtifacts, "schema") {
		t.Fatalf("failed_artifacts = %v, must name schema", res.FailedArtifacts)
	}
}

func TestTamperedSignerPubkeyFails(t *testing.T) {
	dir := sealBundle(t, nil)
	editManifest(t, dir, func(m map[string]any) {
		m["signer_pubkey"] = flipHexDigit(m["signer_pubkey"].(string))
	})
	res := Verify(dir)
	if res.Valid {
		t.Fatal("bundle with tampered signer_pubkey verified")
	}
	// A substituted key must at minimum break the signature check.
	if !contains(res.FailedArtifacts, "signature") {
		t.Fatalf("failed_artifacts = %v, substituted pubkey must fail the signature", res.FailedArtifacts)
	}
}
