// Command dal-conformance checks driver manifests against the hardware
// requirements of spec §10.3. "Your hardware ships when it passes" — this
// binary is the gate the M9 hardware program builds toward, exercised for
// years against third-party gear first.
//
// Usage:
//
//	dal-conformance <manifest.yaml...>
//
// Every violation in every manifest is reported on stderr, one line each,
// prefixed with the file path. Exit 1 if any manifest fails, 2 on usage
// error, 0 only when every manifest conforms.
package main

import (
	"fmt"
	"os"

	"github.com/mrsdlmnbr/aregis/tools/dal-conformance/conformance"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: dal-conformance <manifest.yaml...>")
		os.Exit(2)
	}
	failed := false
	for _, path := range os.Args[1:] {
		for _, v := range conformance.CheckFile(path) {
			failed = true
			fmt.Fprintf(os.Stderr, "%s: [%s] %s\n", path, v.Rule, v.Msg)
		}
	}
	if failed {
		os.Exit(1)
	}
}
