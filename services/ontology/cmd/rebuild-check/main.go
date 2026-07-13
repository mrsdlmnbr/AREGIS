// rebuild-check is the M0 stand-in for `make rebuild-from-log` (spec §7.2,
// §9.4 acceptance): the twin must be rebuildable from the log alone, and two
// independent rebuilds must be byte-identical.
//
// It generates the same synthetic 200-event log twice from a fixed seed
// (timestamps computed from a fixed base — no wall clock, axiom A7), folds
// each independently from the empty twin, and byte-compares the canonical
// marshals. Exit 0 on identical; exit 1 with a diff summary. A diff here
// means the fold or the canonical marshal is nondeterministic — that is a
// broken assurance story, not a flaky check. Do not rerun it until it
// passes; find out why (CLAUDE.md).
package main

import (
	"bytes"
	"fmt"
	"os"

	"github.com/mrsdlmnbr/aregis/services/ontology/internal/synthlog"
	"github.com/mrsdlmnbr/aregis/services/ontology/twin"
)

const (
	seed   = 42
	events = 200
)

func main() {
	// Two fully independent passes: generator → log → fold → canonical
	// bytes. Sharing nothing between the passes is the point.
	a := twin.MarshalCanonical(twin.Rebuild(synthlog.Log(seed, events)))
	b := twin.MarshalCanonical(twin.Rebuild(synthlog.Log(seed, events)))

	if bytes.Equal(a, b) {
		fmt.Printf("rebuild-check OK: %d events folded twice, %d canonical bytes, byte-identical\n",
			events, len(a))
		return
	}

	fmt.Fprintf(os.Stderr, "rebuild-check FAIL: twin rebuild is not deterministic\n")
	fmt.Fprintf(os.Stderr, "  pass A: %d bytes\n  pass B: %d bytes\n", len(a), len(b))
	if i := firstDiff(a, b); i >= 0 {
		fmt.Fprintf(os.Stderr, "  first differing byte at offset %d (A=0x%02x B=0x%02x)\n",
			i, byteAt(a, i), byteAt(b, i))
	}
	os.Exit(1)
}

// firstDiff returns the first offset at which a and b differ, or -1.
func firstDiff(a, b []byte) int {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i] != b[i] {
			return i
		}
	}
	if len(a) != len(b) {
		return n
	}
	return -1
}

func byteAt(s []byte, i int) byte {
	if i < len(s) {
		return s[i]
	}
	return 0
}
