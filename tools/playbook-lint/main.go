// Command playbook-lint is the static safety verifier for playbooks
// (spec §11.1; docs/contracts/sim-cli.md §4). It runs in CI and at load
// time: a playbook that fails here never reaches a live property.
//
// Usage:
//
//	playbook-lint <playbook.yaml...>
//
// Run from the repo root — tests: scenario paths resolve against the
// current working directory. Every violation in every file is reported on
// stderr, one line each, prefixed with the file path. Exit 1 if any file
// fails, 2 on usage error, 0 only when every file is clean.
package main

import (
	"fmt"
	"os"

	"github.com/mrsdlmnbr/aregis/tools/playbook-lint/lint"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: playbook-lint <playbook.yaml...>")
		os.Exit(2)
	}
	failed := false
	for _, path := range os.Args[1:] {
		for _, v := range lint.File(path, ".") {
			failed = true
			fmt.Fprintf(os.Stderr, "%s: %s\n", path, v.Msg)
		}
	}
	if failed {
		os.Exit(1)
	}
}
