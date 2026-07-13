package conformance

import "testing"

func base() Manifest {
	return Manifest{
		Driver:       "onvif",
		Capabilities: []string{"VIDEO_SOURCE", "AUDIO_SOURCE"},
		ClockSync:    "NTP",
	}
}

func rules(vs []Violation) map[string]bool {
	m := map[string]bool{}
	for _, v := range vs {
		m[v.Rule] = true
	}
	return m
}

func TestModestThirdPartyCameraPasses(t *testing.T) {
	// Third-party gear implements a subset and passes with an honest
	// manifest (axiom A6).
	if vs := Check(base()); len(vs) != 0 {
		t.Fatalf("expected pass, got %v", vs)
	}
}

func TestOwnHardwareSupersetPasses(t *testing.T) {
	m := Manifest{
		Driver: "aegis-cam-1",
		Capabilities: []string{
			"VIDEO_SOURCE", "AUDIO_SOURCE", "EDGE_EMBEDDER",
			"SIGNED_CAPTURE", "PTP_CLOCK", "LOCAL_BUFFER",
		},
		ClockSync:      "PTP",
		SignsAtCapture: true,
		LocalBuffer:    true,
	}
	if vs := Check(m); len(vs) != 0 {
		t.Fatalf("the M9 hardware manifest must pass, got %v", vs)
	}
}

func TestSignedCaptureCannotBeALie(t *testing.T) {
	m := base()
	m.Capabilities = append(m.Capabilities, "SIGNED_CAPTURE")
	m.SignsAtCapture = false
	if !rules(Check(m))["signed-capture"] {
		t.Fatal("SIGNED_CAPTURE without signs_at_capture must be rejected (§10.3.1)")
	}
}

func TestPtpClockRequiresPtp(t *testing.T) {
	m := base()
	m.Capabilities = append(m.Capabilities, "PTP_CLOCK")
	m.ClockSync = "NTP"
	if !rules(Check(m))["ptp-clock"] {
		t.Fatal("PTP_CLOCK with NTP sync must be rejected — hard time sync means PTP (§10.3.2)")
	}
}

func TestLocalBufferMustSurviveTheCutWire(t *testing.T) {
	m := base()
	m.Capabilities = append(m.Capabilities, "LOCAL_BUFFER")
	m.LocalBuffer = false
	if !rules(Check(m))["local-buffer"] {
		t.Fatal("LOCAL_BUFFER without a buffer must be rejected (§10.3.4)")
	}
}

func TestMobileAssetNeedsMissionApiAndOnboardFence(t *testing.T) {
	m := base()
	m.Capabilities = []string{"MOBILE_ASSET"}
	m.MissionAPI = false
	m.OnboardGeofenceEnforcement = false
	got := rules(Check(m))
	if !got["mission-api"] {
		t.Fatal("a joystick robot must be rejected (§10.3.5)")
	}
	if !got["onboard-geofence"] {
		t.Fatal("a mobile asset without onboard geofence enforcement must be rejected (§3.2)")
	}
}

func TestUnknownCapabilityIsRejected(t *testing.T) {
	m := base()
	m.Capabilities = append(m.Capabilities, "FACE_SEARCH_EXTERNAL")
	if !rules(Check(m))["unknown-capability"] {
		t.Fatal("the capability set is closed (§10.1)")
	}
}

func TestEveryViolationIsReportedNotJustTheFirst(t *testing.T) {
	m := Manifest{
		Driver:       "Bad Driver!",
		Capabilities: []string{"MOBILE_ASSET", "PTP_CLOCK", "NOT_A_THING"},
		ClockSync:    "sundial",
	}
	got := rules(Check(m))
	for _, want := range []string{"driver-id", "unknown-capability", "clock-sync", "ptp-clock", "mission-api", "onboard-geofence"} {
		if !got[want] {
			t.Fatalf("missing violation %q in %v", want, got)
		}
	}
}

func TestUnparseableManifestFailsClosed(t *testing.T) {
	vs := CheckBytes([]byte("driver: [unterminated"))
	if len(vs) == 0 || vs[0].Rule != "manifest" {
		t.Fatalf("garbage manifest must fail closed, got %v", vs)
	}
}
