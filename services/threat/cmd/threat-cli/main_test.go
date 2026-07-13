package main

import (
	"bytes"
	"strings"
	"testing"
)

// s001Stdin is the literal stdin block from docs/contracts/sim-cli.md §1.
const s001Stdin = `{
  "schema": "aegis.sim.threat/v1",
  "object_class": "PERSON",
  "identity_known": false,
  "zone_class": "GROUNDS",
  "posture": "AWAY",
  "local_hour": 3,
  "expected": false,
  "entity_confidence": 0.9936,
  "distinct_sensors": 3,
  "mesh_corroborations": 0,
  "dwell_seconds": 0.0,
  "pol": {
    "available": true,
    "anomaly": 0.8,
    "note": "last unexpected perimeter entity: 41 days ago"
  },
  "all_contributing_unattested": false
}
`

// s001Stdout is the S-001 golden output as this CLI encodes it: one line,
// Go encoding/json default float64 formatting (contract §1). Byte-identical
// forever — the Rust sim harness asserts these bytes.
const s001Stdout = `{"schema":"aegis.sim.threat/v1","score":6.6,"severity":4,` +
	`"receipt":[` +
	`{"name":"base","input":"PERSON/unknown @ GROUNDS","weight":1,"contribution":2.4,"note":""},` +
	`{"name":"posture_amplifier","input":"AWAY @ 03h (night)","weight":2,"contribution":2.4,"note":""},` +
	`{"name":"expectation_discount","input":"no expectation matched","weight":0,"contribution":0,"note":""},` +
	`{"name":"pol_anomaly","input":"last unexpected perimeter entity: 41 days ago","weight":1.5,"contribution":1.2,"note":""},` +
	`{"name":"corroboration","input":"3 sensors, 0 mesh","weight":1,"contribution":0.6,"note":""},` +
	`{"name":"dwell","input":"0.0 s","weight":0.5,"contribution":0,"note":""},` +
	`{"name":"known_benign","input":"none","weight":1,"contribution":0,"note":""}` +
	`],"pol_note":"last unexpected perimeter entity: 41 days ago"}` + "\n"

func TestScoreS001GoldenBytes(t *testing.T) {
	var stdout, stderr bytes.Buffer
	code := run([]string{"score"}, strings.NewReader(s001Stdin), &stdout, &stderr)
	if code != 0 {
		t.Fatalf("exit = %d, want 0; stderr: %s", code, stderr.String())
	}
	if got := stdout.String(); got != s001Stdout {
		t.Errorf("stdout mismatch:\n got: %s\nwant: %s", got, s001Stdout)
	}
	if stderr.Len() != 0 {
		t.Errorf("stderr not empty: %s", stderr.String())
	}
}

func TestScoreDeterministic(t *testing.T) {
	var first bytes.Buffer
	run([]string{"score"}, strings.NewReader(s001Stdin), &first, &bytes.Buffer{})
	var second bytes.Buffer
	run([]string{"score"}, strings.NewReader(s001Stdin), &second, &bytes.Buffer{})
	if !bytes.Equal(first.Bytes(), second.Bytes()) {
		t.Errorf("same stdin produced different stdout")
	}
}

func TestScoreRejectsBadInput(t *testing.T) {
	cases := map[string]struct {
		args  []string
		stdin string
	}{
		"wrong schema":    {[]string{"score"}, `{"schema":"nope","object_class":"PERSON","zone_class":"GROUNDS","posture":"AWAY","local_hour":3,"pol":{"available":true,"anomaly":0.1,"note":""}}`},
		"unknown posture": {[]string{"score"}, strings.Replace(s001Stdin, `"AWAY"`, `"PANIC"`, 1)},
		"not json":        {[]string{"score"}, `severity 5 please`},
		"unknown field":   {[]string{"score"}, `{"schema":"aegis.sim.threat/v1","force_rung":7}`},
		"trailing data":   {[]string{"score"}, s001Stdin + `{"schema":"aegis.sim.threat/v1"}`},
		"no subcommand":   {nil, s001Stdin},
		"bad subcommand":  {[]string{"launch"}, s001Stdin},
	}
	for name, tc := range cases {
		t.Run(name, func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			code := run(tc.args, strings.NewReader(tc.stdin), &stdout, &stderr)
			if code != 1 {
				t.Errorf("exit = %d, want 1", code)
			}
			if stderr.Len() == 0 {
				t.Errorf("stderr empty; failures must be loud")
			}
			if stdout.Len() != 0 {
				t.Errorf("stdout not empty on failure: %s", stdout.String())
			}
		})
	}
}
