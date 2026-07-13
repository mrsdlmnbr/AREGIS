package seal

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// RFC 8032 test-vector seed. Sim/test-only: production signing happens in
// the appliance HSM and no seed ever appears outside sim fixtures (spec §19).
const testSeedHex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"

func b64(s string) string { return base64.StdEncoding.EncodeToString([]byte(s)) }

func testRequest() Request {
	return Request{
		Schema:         Schema,
		IncidentID:     "S-001/alert-1",
		PropertyID:     "ridgeline",
		SealedAt:       "2026-03-14T03:11:45.500Z",
		SealedBy:       "appliance:ridgeline",
		EventIDs:       []string{"evt-aaa", "evt-bbb"},
		AuditRecordIDs: []string{"aud-ccc"},
		Media: []RequestMedia{
			{Name: "frame-sig-1.bin", B64: b64("frame one bytes")},
			{Name: "clip-2.bin", B64: b64("clip two bytes")},
		},
		SigningKeySeedHex: testSeedHex,
	}
}

func pair(t *testing.T, l, r [sha256.Size]byte) [sha256.Size]byte {
	t.Helper()
	return sha256.Sum256(append(append([]byte{}, l[:]...), r[:]...))
}

func TestMerkleRootEmptyIsError(t *testing.T) {
	if _, err := MerkleRoot(nil); err == nil {
		t.Fatal("empty tree must be an error, got nil")
	}
}

func TestMerkleRootSingleLeaf(t *testing.T) {
	leaf := sha256.Sum256([]byte("only"))
	root, err := MerkleRoot([][sha256.Size]byte{leaf})
	if err != nil {
		t.Fatal(err)
	}
	if root != leaf {
		t.Fatalf("single leaf must be the root unchanged: got %x want %x", root, leaf)
	}
}

func TestMerkleRootPair(t *testing.T) {
	l0 := sha256.Sum256([]byte("a"))
	l1 := sha256.Sum256([]byte("b"))
	root, err := MerkleRoot([][sha256.Size]byte{l0, l1})
	if err != nil {
		t.Fatal(err)
	}
	if want := pair(t, l0, l1); root != want {
		t.Fatalf("got %x want sha256(l0||l1)=%x", root, want)
	}
}

// Odd leaf count: the unpaired last node is promoted unchanged, per contract.
func TestMerkleRootOddPromotion(t *testing.T) {
	l0 := sha256.Sum256([]byte("a"))
	l1 := sha256.Sum256([]byte("b"))
	l2 := sha256.Sum256([]byte("c"))
	root, err := MerkleRoot([][sha256.Size]byte{l0, l1, l2})
	if err != nil {
		t.Fatal(err)
	}
	// level 1: [h(l0||l1), l2]  →  root: h(h(l0||l1) || l2)
	if want := pair(t, pair(t, l0, l1), l2); root != want {
		t.Fatalf("3 leaves: got %x want %x", root, want)
	}

	l3 := sha256.Sum256([]byte("d"))
	l4 := sha256.Sum256([]byte("e"))
	root5, err := MerkleRoot([][sha256.Size]byte{l0, l1, l2, l3, l4})
	if err != nil {
		t.Fatal(err)
	}
	// level 1: [h01, h23, l4] → level 2: [h(h01||h23), l4] → root: h(.. || l4)
	h01 := pair(t, l0, l1)
	h23 := pair(t, l2, l3)
	if want := pair(t, pair(t, h01, h23), l4); root5 != want {
		t.Fatalf("5 leaves: got %x want %x", root5, want)
	}
}

func TestMerkleRootDoesNotMutateInput(t *testing.T) {
	l0 := sha256.Sum256([]byte("a"))
	l1 := sha256.Sum256([]byte("b"))
	l2 := sha256.Sum256([]byte("c"))
	leaves := [][sha256.Size]byte{l0, l1, l2}
	if _, err := MerkleRoot(leaves); err != nil {
		t.Fatal(err)
	}
	if leaves[0] != l0 || leaves[1] != l1 || leaves[2] != l2 {
		t.Fatal("MerkleRoot mutated its input slice")
	}
}

func TestSealRejects(t *testing.T) {
	cases := []struct {
		name    string
		mutate  func(*Request)
		errPart string
	}{
		{"wrong schema", func(r *Request) { r.Schema = "aegis.sim.evidence/v0" }, "schema"},
		{"missing signing key seed", func(r *Request) { r.SigningKeySeedHex = "" }, "refusing to invent"},
		{"seed not hex", func(r *Request) { r.SigningKeySeedHex = strings.Repeat("zz", 32) }, "hex"},
		{"seed wrong length", func(r *Request) { r.SigningKeySeedHex = "abcd" }, "32 bytes"},
		{"empty tree", func(r *Request) { r.Media = nil; r.EventIDs = nil; r.AuditRecordIDs = nil }, "empty tree"},
		{"bad media base64", func(r *Request) { r.Media[0].B64 = "!!not-base64!!" }, "base64"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			req := testRequest()
			tc.mutate(&req)
			_, err := Seal(req, t.TempDir())
			if err == nil {
				t.Fatal("expected error, got nil")
			}
			if !strings.Contains(err.Error(), tc.errPart) {
				t.Fatalf("error %q does not mention %q", err, tc.errPart)
			}
		})
	}
}

// The full roundtrip at the seal level: recompute the leaves and root by
// hand from the request, compare to the manifest, and verify the Ed25519
// signature over the 32 raw root bytes. (The independent binary-level check
// lives in tools/evidence-verify's tests.)
func TestSealBundleRoundtrip(t *testing.T) {
	dir := t.TempDir()
	req := testRequest()
	stdoutBytes, err := Seal(req, dir)
	if err != nil {
		t.Fatal(err)
	}

	onDisk, err := os.ReadFile(filepath.Join(dir, "manifest.json"))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(stdoutBytes, onDisk) {
		t.Fatal("returned manifest bytes differ from manifest.json on disk")
	}

	var man Manifest
	if err := json.Unmarshal(onDisk, &man); err != nil {
		t.Fatal(err)
	}
	if man.Schema != Schema || man.IncidentID != req.IncidentID ||
		man.PropertyID != req.PropertyID || man.SealedAt != req.SealedAt ||
		man.SealedBy != req.SealedBy {
		t.Fatalf("manifest header does not echo request: %+v", man)
	}

	// Media files: content-addressed, hash is the filename and matches the
	// manifest entry.
	wantMedia := map[string]string{
		"frame-sig-1.bin": "frame one bytes",
		"clip-2.bin":      "clip two bytes",
	}
	var leaves [][sha256.Size]byte
	for i, m := range man.Media {
		content, ok := wantMedia[m.Name]
		if !ok {
			t.Fatalf("unexpected media entry %q", m.Name)
		}
		sum := sha256.Sum256([]byte(content))
		if m.SHA256 != hex.EncodeToString(sum[:]) {
			t.Fatalf("media[%d] %q: manifest sha %s != recomputed %x", i, m.Name, m.SHA256, sum)
		}
		data, err := os.ReadFile(filepath.Join(dir, "media", m.SHA256))
		if err != nil {
			t.Fatalf("media file for %q missing: %v", m.Name, err)
		}
		if string(data) != content {
			t.Fatalf("media file for %q has wrong bytes", m.Name)
		}
		leaves = append(leaves, sum)
	}
	for _, id := range req.EventIDs {
		leaves = append(leaves, sha256.Sum256([]byte(id)))
	}
	for _, id := range req.AuditRecordIDs {
		leaves = append(leaves, sha256.Sum256([]byte(id)))
	}
	root, err := MerkleRoot(leaves)
	if err != nil {
		t.Fatal(err)
	}
	if man.MerkleRoot != hex.EncodeToString(root[:]) {
		t.Fatalf("manifest merkle_root %s != recomputed %x", man.MerkleRoot, root)
	}

	pub, err := hex.DecodeString(man.SignerPubkey)
	if err != nil || len(pub) != ed25519.PublicKeySize {
		t.Fatalf("bad signer_pubkey %q: %v", man.SignerPubkey, err)
	}
	sig, err := hex.DecodeString(man.Signature)
	if err != nil || len(sig) != ed25519.SignatureSize {
		t.Fatalf("bad signature %q: %v", man.Signature, err)
	}
	if !ed25519.Verify(ed25519.PublicKey(pub), root[:], sig) {
		t.Fatal("Ed25519 signature over the raw root bytes does not verify")
	}

	// The pubkey must be the one derived from the request's seed — the CLI
	// must never substitute a key of its own.
	seed, _ := hex.DecodeString(testSeedHex)
	wantPub := ed25519.NewKeyFromSeed(seed).Public().(ed25519.PublicKey)
	if !bytes.Equal(pub, wantPub) {
		t.Fatal("signer_pubkey is not derived from the request seed")
	}
}

// Odd total leaf count (2 media + 2 events + 1 audit = 5) through the full
// Seal path exercises node promotion beyond the pure MerkleRoot tests.
func TestSealOddLeafCount(t *testing.T) {
	req := testRequest() // 2 + 2 + 1 = 5 leaves
	if got := len(req.Media) + len(req.EventIDs) + len(req.AuditRecordIDs); got%2 == 0 {
		t.Fatalf("test setup: want odd leaf count, got %d", got)
	}
	if _, err := Seal(req, t.TempDir()); err != nil {
		t.Fatal(err)
	}
}

func TestSealDeterministic(t *testing.T) {
	req := testRequest()
	a, err := Seal(req, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	b, err := Seal(req, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(a, b) {
		t.Fatal("sealing the same request twice produced different manifest bytes")
	}
}

// Pin the contract's exact JSON field names — these are wire format, and the
// standalone verifier parses them with its own independent struct.
func TestManifestFieldNamesAreContract(t *testing.T) {
	man, err := Seal(testRequest(), t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]json.RawMessage
	if err := json.Unmarshal(man, &m); err != nil {
		t.Fatal(err)
	}
	for _, k := range []string{
		"schema", "incident_id", "property_id", "sealed_at", "sealed_by",
		"event_ids", "audit_record_ids", "media", "merkle_root", "signature",
		"signer_pubkey",
	} {
		if _, ok := m[k]; !ok {
			t.Errorf("manifest missing contract field %q", k)
		}
	}
	if len(m) != 11 {
		t.Errorf("manifest has %d fields, contract has 11", len(m))
	}
	var media []map[string]json.RawMessage
	if err := json.Unmarshal(m["media"], &media); err != nil {
		t.Fatal(err)
	}
	for _, entry := range media {
		for _, k := range []string{"name", "sha256"} {
			if _, ok := entry[k]; !ok {
				t.Errorf("media entry missing contract field %q", k)
			}
		}
	}
}
