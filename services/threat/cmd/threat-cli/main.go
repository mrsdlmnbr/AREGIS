// threat-cli is the deterministic sim entrypoint for the severity scorer
// (docs/contracts/sim-cli.md §1). JSON in on stdin, JSON out on stdout —
// one line, stable forever — human-readable errors on stderr, exit 1 on any
// failure. It never reads a clock: time is an input (axiom A7).
package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/mrsdlmnbr/aregis/services/threat/scoring"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	if len(args) != 1 || args[0] != "score" {
		fmt.Fprintln(stderr, "usage: threat-cli score < input.json > output.json")
		return 1
	}

	dec := json.NewDecoder(stdin)
	// Fail closed: a field this scorer does not understand must not be
	// silently ignored — it could be the one that mattered.
	dec.DisallowUnknownFields()
	var in scoring.Input
	if err := dec.Decode(&in); err != nil {
		fmt.Fprintf(stderr, "threat-cli: bad input: %v\n", err)
		return 1
	}
	if _, err := dec.Token(); !errors.Is(err, io.EOF) {
		fmt.Fprintln(stderr, "threat-cli: trailing data after input object")
		return 1
	}
	if in.Schema != scoring.SchemaV1 {
		fmt.Fprintf(stderr, "threat-cli: schema must be %q, got %q\n", scoring.SchemaV1, in.Schema)
		return 1
	}

	out, err := scoring.Score(in)
	if err != nil {
		fmt.Fprintf(stderr, "threat-cli: %v\n", err)
		return 1
	}

	b, err := json.Marshal(out)
	if err != nil {
		fmt.Fprintf(stderr, "threat-cli: encoding output: %v\n", err)
		return 1
	}
	b = append(b, '\n')
	if _, err := stdout.Write(b); err != nil {
		fmt.Fprintf(stderr, "threat-cli: writing output: %v\n", err)
		return 1
	}
	return 0
}
