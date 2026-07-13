// sim-camera — the reference DAL driver: deterministic, fixture-driven,
// zero hardware (spec §10.2, §19 "an engineer must be able to run the
// entire product on a laptop, on day one").
//
// Reads a fixture JSON of frames and emits provenance-stamped envelope JSON
// lines on stdout. Timestamps come FROM THE FIXTURE (axiom A7) — this
// process never reads a clock, so its output is byte-identical forever.
// The M1 version of this driver speaks the local gRPC contract to gateway;
// the emission model (fixture in, stamped envelopes out) stays the same.
//
// Doctrine notes that apply to every driver, demonstrated here:
//   - no authority_ref in the fixture ⇒ the frame is REFUSED, not defaulted
//     (spec §3.4: accessible is not authorized);
//   - this driver has no notion of an asset command. Command() may not move
//     an asset (§10.2); archlint fails the build if a driver references
//     asset command types.
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
)

type Fixture struct {
	DeviceID     string  `json:"device_id"`
	Authority    string  `json:"authority"`
	AuthorityRef string  `json:"authority_ref"`
	Attested     bool    `json:"attested"`
	Frames       []Frame `json:"frames"`
}

type Frame struct {
	CapturedAt string `json:"captured_at"` // RFC3339, from the fixture — never a clock
	PayloadB64 string `json:"payload_b64"`
	Note       string `json:"note,omitempty"`
}

type Emitted struct {
	SourceID     string `json:"source_id"`
	Authority    string `json:"authority"`
	AuthorityRef string `json:"authority_ref"`
	Attested     bool   `json:"attested"`
	CapturedAt   string `json:"captured_at"`
	PayloadType  string `json:"payload_type"`
	PayloadB64   string `json:"payload_b64"`
	FrameSHA256  string `json:"frame_sha256"` // chain of custody starts HERE
}

func run(fixturePath string, out *json.Encoder) error {
	data, err := os.ReadFile(fixturePath)
	if err != nil {
		return err
	}
	var f Fixture
	if err := json.Unmarshal(data, &f); err != nil {
		return fmt.Errorf("fixture is not valid JSON: %w", err)
	}
	if f.AuthorityRef == "" {
		// No authority, no ingestion. A driver that "helpfully" defaults
		// this field is the first step toward the headline that ends the
		// company (spec §3.4).
		return fmt.Errorf("fixture for %s has no authority_ref — refusing to emit", f.DeviceID)
	}
	for _, fr := range f.Frames {
		sum := sha256.Sum256([]byte(fr.PayloadB64))
		if err := out.Encode(Emitted{
			SourceID:     f.DeviceID,
			Authority:    f.Authority,
			AuthorityRef: f.AuthorityRef,
			Attested:     f.Attested,
			CapturedAt:   fr.CapturedAt,
			PayloadType:  "video/frame",
			PayloadB64:   fr.PayloadB64,
			FrameSHA256:  hex.EncodeToString(sum[:]),
		}); err != nil {
			return err
		}
	}
	return nil
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: sim-camera <fixture.json>")
		os.Exit(2)
	}
	if err := run(os.Args[1], json.NewEncoder(os.Stdout)); err != nil {
		fmt.Fprintln(os.Stderr, "sim-camera:", err)
		os.Exit(1)
	}
}
