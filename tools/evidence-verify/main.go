// Command evidence-verify is the standalone third-party verifier for sealed
// AEGIS evidence bundles (spec §9.9, docs/contracts/sim-cli.md §3).
//
// INDEPENDENCE OF THE VERIFIER FROM THE SEALER IS THE POINT. Police,
// insurers, and opposing counsel run this binary and nothing else. It
// therefore imports only the Go standard library and deliberately
// re-implements the manifest schema, the content addressing, and the Merkle
// fold from the written contract instead of sharing code with
// services/evidence. Do not "deduplicate" it against the sealer: a bug
// shared by both sides would become invisible, which is exactly the failure
// this tool exists to expose. It reads one directory and never touches the
// network — no phoning home.
//
// Usage:
//
//	evidence-verify <bundle-dir>
//
// stdout: {"valid":true,"failed_artifacts":[]} and exit 0, or
// {"valid":false,"failed_artifacts":[...]} naming EVERY failing artifact or
// manifest field (not just the first) and exit 1.
package main

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

const schemaID = "aegis.sim.evidence/v1"

// Result is the verifier's verdict, printed as JSON on stdout.
type Result struct {
	Valid           bool     `json:"valid"`
	FailedArtifacts []string `json:"failed_artifacts"`
}

// manifest mirrors docs/contracts/sim-cli.md §2 "Bundle layout". Declared
// here, not imported — see the package comment.
type manifest struct {
	Schema         string          `json:"schema"`
	IncidentID     string          `json:"incident_id"`
	PropertyID     string          `json:"property_id"`
	SealedAt       string          `json:"sealed_at"`
	SealedBy       string          `json:"sealed_by"`
	EventIDs       []string        `json:"event_ids"`
	AuditRecordIDs []string        `json:"audit_record_ids"`
	Media          []manifestMedia `json:"media"`
	MerkleRoot     string          `json:"merkle_root"`
	Signature      string          `json:"signature"`
	SignerPubkey   string          `json:"signer_pubkey"`
}

type manifestMedia struct {
	Name   string `json:"name"`
	SHA256 string `json:"sha256"`
}

// merkleRoot re-implements the normative fold of docs/contracts/sim-cli.md
// §2, independently of the sealer: parent = sha256(left || right); an
// unpaired last node is promoted unchanged to the next level. Caller
// guarantees at least one leaf.
func merkleRoot(leaves [][sha256.Size]byte) [sha256.Size]byte {
	level := append([][sha256.Size]byte(nil), leaves...)
	for len(level) > 1 {
		next := make([][sha256.Size]byte, 0, (len(level)+1)/2)
		for i := 0; i < len(level); i += 2 {
			if i+1 == len(level) {
				next = append(next, level[i])
				continue
			}
			buf := make([]byte, 0, 2*sha256.Size)
			buf = append(buf, level[i][:]...)
			buf = append(buf, level[i+1][:]...)
			next = append(next, sha256.Sum256(buf))
		}
		level = next
	}
	return level[0]
}

// Verify checks a sealed bundle end to end and collects EVERY failure —
// a partial verdict would let a tampered artifact hide behind an earlier
// one. Exported so tests exercise the real verification logic without
// exec'ing the binary.
func Verify(bundleDir string) Result {
	failed := []string{}
	fail := func(name string) { failed = append(failed, name) }

	raw, err := os.ReadFile(filepath.Join(bundleDir, "manifest.json"))
	if err != nil {
		return Result{Valid: false, FailedArtifacts: []string{"manifest.json"}}
	}
	var man manifest
	if err := json.Unmarshal(raw, &man); err != nil {
		return Result{Valid: false, FailedArtifacts: []string{"manifest.json"}}
	}
	if man.Schema != schemaID {
		fail("schema")
	}

	// Media: recompute every artifact's SHA-256 from the bytes on disk and
	// compare it to the manifest entry AND the filename. The bundle is
	// content-addressed — the manifest hash is the filename we open, so one
	// comparison covers both claims.
	//
	// The Merkle rebuild below uses the manifest's hashes (the signed
	// statement), while this on-disk comparison catches content tamper.
	// That split is what makes a flipped media byte name exactly that
	// artifact and nothing else.
	leaves := make([][sha256.Size]byte, 0, len(man.Media)+len(man.EventIDs)+len(man.AuditRecordIDs))
	for _, m := range man.Media {
		var claimed [sha256.Size]byte
		claimedBytes, err := hex.DecodeString(m.SHA256)
		claimOK := err == nil && len(claimedBytes) == sha256.Size
		if claimOK {
			copy(claimed[:], claimedBytes)
		}
		leaves = append(leaves, claimed)
		if !claimOK {
			fail(m.Name)
			continue
		}
		data, err := os.ReadFile(filepath.Join(bundleDir, "media", m.SHA256))
		if err != nil {
			fail(m.Name) // artifact missing from the bundle
			continue
		}
		if sha256.Sum256(data) != claimed {
			fail(m.Name) // bytes on disk do not match manifest/filename
		}
	}

	// Leaves in normative order: media, then sha256(utf8(event_id)) per
	// event, then sha256(utf8(audit_record_id)) per audit record.
	for _, id := range man.EventIDs {
		leaves = append(leaves, sha256.Sum256([]byte(id)))
	}
	for _, id := range man.AuditRecordIDs {
		leaves = append(leaves, sha256.Sum256([]byte(id)))
	}

	claimedRoot, rootErr := hex.DecodeString(man.MerkleRoot)
	rootDecodes := rootErr == nil && len(claimedRoot) == sha256.Size
	if len(leaves) == 0 {
		fail("merkle_root") // empty tree is invalid by contract
	} else {
		rebuilt := merkleRoot(leaves)
		if !rootDecodes || !bytes.Equal(claimedRoot, rebuilt[:]) {
			fail("merkle_root")
		}
	}

	pub, pubErr := hex.DecodeString(man.SignerPubkey)
	if pubErr != nil || len(pub) != ed25519.PublicKeySize {
		fail("signer_pubkey")
		pub = nil
	}
	sig, sigErr := hex.DecodeString(man.Signature)
	switch {
	case sigErr != nil || len(sig) != ed25519.SignatureSize:
		fail("signature")
	case pub != nil && rootDecodes:
		// The signature covers the 32 raw bytes of the manifest's stated
		// root. Whether that root matches the bundle's content is the
		// separate merkle_root check above.
		if !ed25519.Verify(ed25519.PublicKey(pub), claimedRoot, sig) {
			fail("signature")
		}
	}

	return Result{Valid: len(failed) == 0, FailedArtifacts: failed}
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: evidence-verify <bundle-dir>")
		os.Exit(2)
	}
	res := Verify(os.Args[1])
	out, err := json.Marshal(res)
	if err != nil {
		fmt.Fprintf(os.Stderr, "evidence-verify: encoding result: %v\n", err)
		os.Exit(2)
	}
	out = append(out, '\n')
	if _, err := os.Stdout.Write(out); err != nil {
		fmt.Fprintf(os.Stderr, "evidence-verify: writing stdout: %v\n", err)
		os.Exit(2)
	}
	if !res.Valid {
		os.Exit(1)
	}
}
