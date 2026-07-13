// Package seal builds sealed evidence bundles — the EvidencePackage of spec
// §9.9: content-addressed media, a Merkle tree over media hashes + the
// contributing event IDs + the audit record IDs, root signed with Ed25519.
//
// Contract: docs/contracts/sim-cli.md §2. The Merkle construction there is
// normative. tools/evidence-verify re-implements it independently, on
// purpose — do not extract a shared helper between the two; the whole point
// of the verifier is that it does not trust this package.
//
// Time is an input (axiom A7): sealed_at arrives in the request. Nothing in
// this package reads a clock.
package seal

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

// Schema is the only request/manifest schema this sealer accepts or emits.
const Schema = "aegis.sim.evidence/v1"

// Request is the seal request read from stdin by evidence-cli
// (docs/contracts/sim-cli.md §2 "Input"). Field names are contract, not
// style — do not rename the JSON tags.
type Request struct {
	Schema         string         `json:"schema"`
	IncidentID     string         `json:"incident_id"`
	PropertyID     string         `json:"property_id"`
	SealedAt       string         `json:"sealed_at"`
	SealedBy       string         `json:"sealed_by"`
	EventIDs       []string       `json:"event_ids"`
	AuditRecordIDs []string       `json:"audit_record_ids"`
	Media          []RequestMedia `json:"media"`

	// SigningKeySeedHex is a SIM-ONLY affordance (docs/contracts/sim-cli.md
	// §2): in production the appliance HSM signs and the key never leaves it
	// (spec §19). Seal rejects a request without it — this code path must
	// never invent a key.
	SigningKeySeedHex string `json:"signing_key_seed_hex"`
}

// RequestMedia is one media artifact in a seal request, bytes inline as
// standard base64.
type RequestMedia struct {
	Name string `json:"name"`
	B64  string `json:"b64"`
}

// Manifest is the manifest.json of a sealed bundle
// (docs/contracts/sim-cli.md §2 "Bundle layout"). Field order here is the
// field order in the emitted JSON; it is part of the deterministic output.
type Manifest struct {
	Schema         string          `json:"schema"`
	IncidentID     string          `json:"incident_id"`
	PropertyID     string          `json:"property_id"`
	SealedAt       string          `json:"sealed_at"`
	SealedBy       string          `json:"sealed_by"`
	EventIDs       []string        `json:"event_ids"`
	AuditRecordIDs []string        `json:"audit_record_ids"`
	Media          []ManifestMedia `json:"media"`
	MerkleRoot     string          `json:"merkle_root"`
	Signature      string          `json:"signature"`
	SignerPubkey   string          `json:"signer_pubkey"`
}

// ManifestMedia is one media artifact in the manifest: its request name and
// the SHA-256 (lowercase hex) of its bytes. The hash is also the artifact's
// filename under media/ — content addressing IS the chain of custody.
type ManifestMedia struct {
	Name   string `json:"name"`
	SHA256 string `json:"sha256"`
}

// MerkleRoot folds leaves to a root exactly per docs/contracts/sim-cli.md §2:
// parent = sha256(left || right); an unpaired last node is promoted unchanged
// to the next level. An empty tree is invalid — a seal must attest to at
// least one thing, or it attests to nothing and is worthless in court.
func MerkleRoot(leaves [][sha256.Size]byte) ([sha256.Size]byte, error) {
	var zero [sha256.Size]byte
	if len(leaves) == 0 {
		return zero, errors.New("merkle: empty tree is invalid — a seal needs at least one leaf")
	}
	level := append([][sha256.Size]byte(nil), leaves...)
	for len(level) > 1 {
		next := make([][sha256.Size]byte, 0, (len(level)+1)/2)
		for i := 0; i < len(level); i += 2 {
			if i+1 == len(level) {
				next = append(next, level[i]) // unpaired last node: promoted unchanged
				continue
			}
			buf := make([]byte, 0, 2*sha256.Size)
			buf = append(buf, level[i][:]...)
			buf = append(buf, level[i+1][:]...)
			next = append(next, sha256.Sum256(buf))
		}
		level = next
	}
	return level[0], nil
}

// Seal validates req, builds the Merkle tree (leaves in contract order:
// media hashes, then sha256(utf8(event_id)) per event, then
// sha256(utf8(audit_record_id)) per audit record), signs the 32 raw root
// bytes with Ed25519, and writes the bundle to outDir:
//
//	outDir/manifest.json
//	outDir/media/<sha256-hex>   (one file per artifact)
//
// It returns the exact bytes of manifest.json. Same request in, byte-identical
// manifest out, forever.
func Seal(req Request, outDir string) ([]byte, error) {
	if req.Schema != Schema {
		return nil, fmt.Errorf("unsupported request schema %q (want %q)", req.Schema, Schema)
	}
	if req.SigningKeySeedHex == "" {
		// Doctrine (spec §19, contract §2): the sealer must never invent a
		// signing key. A seal under a key nobody controls is worse than no
		// seal — it looks like custody without being custody. Fail closed.
		return nil, errors.New("seal request carries no signing key seed; refusing to invent one (spec §19)")
	}
	seed, err := hex.DecodeString(req.SigningKeySeedHex)
	if err != nil {
		return nil, fmt.Errorf("signing key seed is not valid hex: %v", err)
	}
	if len(seed) != ed25519.SeedSize {
		return nil, fmt.Errorf("signing key seed must be %d bytes (%d hex chars), got %d bytes",
			ed25519.SeedSize, 2*ed25519.SeedSize, len(seed))
	}
	if len(req.Media)+len(req.EventIDs)+len(req.AuditRecordIDs) == 0 {
		return nil, errors.New("seal request has no media, event ids, or audit record ids: empty tree is invalid")
	}

	type blob struct {
		data []byte
		hash [sha256.Size]byte
	}
	blobs := make([]blob, 0, len(req.Media))
	mediaOut := make([]ManifestMedia, 0, len(req.Media))
	for i, m := range req.Media {
		data, err := base64.StdEncoding.DecodeString(m.B64)
		if err != nil {
			return nil, fmt.Errorf("media[%d] %q: invalid base64: %v", i, m.Name, err)
		}
		h := sha256.Sum256(data)
		blobs = append(blobs, blob{data: data, hash: h})
		mediaOut = append(mediaOut, ManifestMedia{Name: m.Name, SHA256: hex.EncodeToString(h[:])})
	}

	// Leaves in normative order: media, then event ids, then audit record ids.
	leaves := make([][sha256.Size]byte, 0, len(blobs)+len(req.EventIDs)+len(req.AuditRecordIDs))
	for _, b := range blobs {
		leaves = append(leaves, b.hash)
	}
	for _, id := range req.EventIDs {
		leaves = append(leaves, sha256.Sum256([]byte(id)))
	}
	for _, id := range req.AuditRecordIDs {
		leaves = append(leaves, sha256.Sum256([]byte(id)))
	}
	root, err := MerkleRoot(leaves)
	if err != nil {
		return nil, err
	}

	priv := ed25519.NewKeyFromSeed(seed)
	sig := ed25519.Sign(priv, root[:]) // signature is over the 32 raw root bytes, nothing else
	pub := priv.Public().(ed25519.PublicKey)

	man := Manifest{
		Schema:         Schema,
		IncidentID:     req.IncidentID,
		PropertyID:     req.PropertyID,
		SealedAt:       req.SealedAt,
		SealedBy:       req.SealedBy,
		EventIDs:       append([]string{}, req.EventIDs...),
		AuditRecordIDs: append([]string{}, req.AuditRecordIDs...),
		Media:          mediaOut,
		MerkleRoot:     hex.EncodeToString(root[:]),
		Signature:      hex.EncodeToString(sig),
		SignerPubkey:   hex.EncodeToString(pub),
	}
	manifestBytes, err := json.MarshalIndent(man, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("encoding manifest: %v", err)
	}
	manifestBytes = append(manifestBytes, '\n')

	mediaDir := filepath.Join(outDir, "media")
	if err := os.MkdirAll(mediaDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating bundle dir: %v", err)
	}
	for i, b := range blobs {
		p := filepath.Join(mediaDir, mediaOut[i].SHA256)
		if err := os.WriteFile(p, b.data, 0o644); err != nil {
			return nil, fmt.Errorf("writing media %q: %v", mediaOut[i].Name, err)
		}
	}
	if err := os.WriteFile(filepath.Join(outDir, "manifest.json"), manifestBytes, 0o644); err != nil {
		return nil, fmt.Errorf("writing manifest.json: %v", err)
	}
	return manifestBytes, nil
}
