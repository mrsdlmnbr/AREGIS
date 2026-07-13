// Command evidence-cli is the deterministic sim entrypoint for the evidence
// sealer (docs/contracts/sim-cli.md §2). JSON request on stdin, the exact
// bytes of the bundle's manifest.json on stdout, human-readable errors on
// stderr, non-zero exit on failure.
//
// Time is an input (axiom A7): sealed_at arrives in the request. This binary
// never reads the wall clock.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"

	"github.com/mrsdlmnbr/aregis/services/evidence/seal"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	if len(args) < 1 || args[0] != "seal" {
		fmt.Fprintln(stderr, "usage: evidence-cli seal --out <bundle-dir> < seal-request.json > package.json")
		return 2
	}
	fs := flag.NewFlagSet("seal", flag.ContinueOnError)
	fs.SetOutput(stderr)
	out := fs.String("out", "", "bundle output directory (required)")
	if err := fs.Parse(args[1:]); err != nil {
		return 2
	}
	if *out == "" {
		fmt.Fprintln(stderr, "evidence-cli seal: --out <bundle-dir> is required")
		return 2
	}

	raw, err := io.ReadAll(stdin)
	if err != nil {
		fmt.Fprintf(stderr, "evidence-cli seal: reading stdin: %v\n", err)
		return 1
	}
	var req seal.Request
	if err := json.Unmarshal(raw, &req); err != nil {
		fmt.Fprintf(stderr, "evidence-cli seal: request is not valid JSON: %v\n", err)
		return 1
	}
	// All semantic validation — schema id, presence of the signing key seed
	// (never invented here), non-empty tree — lives in seal.Seal and fails
	// closed.
	manifest, err := seal.Seal(req, *out)
	if err != nil {
		fmt.Fprintf(stderr, "evidence-cli seal: %v\n", err)
		return 1
	}
	if _, err := stdout.Write(manifest); err != nil {
		fmt.Fprintf(stderr, "evidence-cli seal: writing stdout: %v\n", err)
		return 1
	}
	return 0
}
