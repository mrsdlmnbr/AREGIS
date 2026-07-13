// Package conformance is the executable form of spec §10.3 — the acceptance
// suite AEGIS hardware must pass before it ships (M9).
//
// The DAL is not merely an integration layer: it is the requirements
// document for the hardware we will build. Third-party gear implements a
// subset of the capability set and may pass with modest manifests; our own
// hardware implements the superset and must pass all of it. Same checker,
// same rules, forever (axiom A6).
package conformance

import (
	"fmt"
	"os"
	"regexp"

	"gopkg.in/yaml.v3"
)

// knownCapabilities is the closed capability set of spec §10.1. Capabilities
// name what a device can do, never what model it is. Extending this set is a
// spec change (new ADR + proto change), not a driver-manifest convenience.
var knownCapabilities = map[string]bool{
	"VIDEO_SOURCE":   true,
	"AUDIO_SOURCE":   true,
	"MOTION_SENSOR":  true,
	"CONTACT_SENSOR": true,
	"GLASSBREAK":     true,
	"ACCESS_POINT":   true,
	"ILLUMINATOR":    true,
	"ANNUNCIATOR":    true,
	"MOBILE_ASSET":   true,
	"NETWORK_SENSOR": true,
	"EXTERNAL_FEED":  true,
	"EDGE_EMBEDDER":  true,
	"SIGNED_CAPTURE": true,
	"PTP_CLOCK":      true,
	"LOCAL_BUFFER":   true,
}

// Manifest is a driver's declaration of what its hardware actually does.
// The checker's job is to catch the manifest that claims a capability the
// declared properties cannot honestly back.
type Manifest struct {
	Driver       string   `yaml:"driver"`
	Capabilities []string `yaml:"capabilities"`
	// ClockSync is PTP or NTP (spec §10.3.2). PTP is required to claim
	// PTP_CLOCK; NTP-only gear may still pass as a lesser device.
	ClockSync      string `yaml:"clock_sync"`
	SignsAtCapture bool   `yaml:"signs_at_capture"`
	LocalBuffer    bool   `yaml:"local_buffer"`
	// MissionAPI means the device exposes mission verbs
	// (goto/orbit/observe/follow/return), never a joystick (spec §10.3.5).
	MissionAPI bool `yaml:"mission_api"`
	// OnboardGeofenceEnforcement: the asset itself holds the fence and
	// hard-stops at the line — the third of the three independent
	// enforcement points (spec §3.2). A mobile asset without it must be
	// rejected here, years before it can be bought.
	OnboardGeofenceEnforcement bool `yaml:"onboard_geofence_enforcement"`
}

// Violation is one conformance failure. Rule is a stable short id so tests
// and CI can match on it without parsing prose.
type Violation struct {
	Rule string
	Msg  string
}

var driverIDRE = regexp.MustCompile(`^[a-z0-9][a-z0-9_-]*$`)

// CheckFile parses and checks one driver manifest file.
func CheckFile(path string) []Violation {
	data, err := os.ReadFile(path)
	if err != nil {
		return []Violation{{Rule: "manifest", Msg: fmt.Sprintf("cannot read manifest: %v", err)}}
	}
	return CheckBytes(data)
}

// CheckBytes parses manifest YAML and runs Check.
func CheckBytes(data []byte) []Violation {
	var m Manifest
	if err := yaml.Unmarshal(data, &m); err != nil {
		return []Violation{{Rule: "manifest", Msg: fmt.Sprintf("spec §10.2: not a parseable driver manifest: %v", err)}}
	}
	return Check(m)
}

// Check validates the structural invariants of spec §10.3 and returns EVERY
// violation, not just the first — a vendor gets the whole gap list in one
// run.
func Check(m Manifest) []Violation {
	var vs []Violation
	add := func(rule, format string, args ...any) {
		vs = append(vs, Violation{Rule: rule, Msg: fmt.Sprintf(format, args...)})
	}

	if !driverIDRE.MatchString(m.Driver) {
		add("driver-id", "spec §10.2: driver id must be a [a-z0-9_-]+ slug, got %q", m.Driver)
	}

	if len(m.Capabilities) == 0 {
		add("no-capabilities", "spec §10.1: a driver must declare at least one capability")
	}
	caps := map[string]bool{}
	for _, c := range m.Capabilities {
		if !knownCapabilities[c] {
			add("unknown-capability", "spec §10.1: %q is not a capability; the set is closed — capabilities, not model numbers, and extending the set is a spec change", c)
			continue
		}
		caps[c] = true
	}

	switch m.ClockSync {
	case "PTP", "NTP":
	case "":
		add("clock-sync", "spec §10.3.2: clock_sync is required (PTP or NTP) — sloppy timestamps silently destroy entity resolution")
	default:
		add("clock-sync", "spec §10.3.2: clock_sync must be PTP or NTP, got %q", m.ClockSync)
	}

	// §10.3.1 — chain of custody begins at the sensor, not the server.
	if caps["SIGNED_CAPTURE"] && !m.SignsAtCapture {
		add("signed-capture", "spec §10.3.1: SIGNED_CAPTURE declared but signs_at_capture is false — signing must happen at capture, in a secure element, or the capability is a lie")
	}

	// §10.3.2 — hard time sync means PTP, not NTP.
	if caps["PTP_CLOCK"] && m.ClockSync != "PTP" {
		add("ptp-clock", "spec §10.3.2: PTP_CLOCK declared but clock_sync is %q — hard time sync means PTP, not NTP", m.ClockSync)
	}

	// §10.3.4 — the wire will be cut; the buffer is what survives it.
	if caps["LOCAL_BUFFER"] && !m.LocalBuffer {
		add("local-buffer", "spec §10.3.4: LOCAL_BUFFER declared but local_buffer is false — a device that cannot record through a cut wire does not have the capability")
	}

	// §10.3.5 — robots expose a mission API, never a joystick, and enforce
	// geofence + ROE onboard. The asset must be INCAPABLE of the forbidden
	// thing (spec §3.2, third enforcement point). Not negotiable per vendor.
	if caps["MOBILE_ASSET"] {
		if !m.MissionAPI {
			add("mission-api", "spec §10.3.5: MOBILE_ASSET requires mission_api — mission verbs (goto/orbit/observe/follow/return), never a joystick")
		}
		if !m.OnboardGeofenceEnforcement {
			add("onboard-geofence", "spec §10.3.5, §3.2: MOBILE_ASSET requires onboard_geofence_enforcement: true — the asset itself must hard-stop at the line, independent of Governor and mission executor")
		}
	}

	return vs
}
