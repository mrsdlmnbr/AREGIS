// Package synthlog builds a deterministic synthetic event log in code, for
// the M0 rebuild-from-log check and the twin fold tests.
//
// Determinism is the point: same seed, same log, forever. Timestamps are
// computed from a fixed base — never the wall clock (axiom A7). math/rand
// with a fixed Source is sequence-stable under the Go 1 compatibility
// promise, so the log is identical across runs and machines.
package synthlog

import (
	"fmt"
	"math/rand"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/timestamppb"
)

// PropertyID is the synthetic estate every generated event belongs to.
const PropertyID = "ridgeline"

// base is the fixed epoch of the synthetic log. A constant, not a clock.
var base = time.Date(2026, time.March, 14, 3, 0, 0, 0, time.UTC)

// Log generates n envelopes deterministically from seed. Payload kinds
// cycle through the five the twin folds — entity upsert, device state
// change, expectation upsert, alert open/dismiss, posture change — plus a
// payload type the M0 fold does not know, to exercise the counted no-op
// path (logs outlive code).
func Log(seed int64, n int) []*aegisv1.Envelope {
	r := rand.New(rand.NewSource(seed))
	log := make([]*aegisv1.Envelope, 0, n)
	for i := 0; i < n; i++ {
		at := base.Add(time.Duration(i) * 500 * time.Millisecond)
		var (
			payloadType string
			topic       string
			msg         proto.Message
		)
		switch i % 7 {
		case 0, 1: // entity upsert — the common case on a live estate
			payloadType, topic = "aegis.v1.Entity", "fusion.entity"
			msg = &aegisv1.Entity{
				Id:            fmt.Sprintf("ent-%02d", r.Intn(24)),
				Class:         aegisv1.ObjectClass(1 + r.Intn(6)),
				IdentityId:    fmt.Sprintf("unknown:%04X", r.Intn(0xFFFF)),
				CurrentZoneId: fmt.Sprintf("zone-%d", r.Intn(4)),
				DwellSeconds:  float32(r.Intn(600)) / 10,
				Confidence:    float32(r.Intn(1000)) / 1000,
				Expected:      r.Intn(3) == 0,
			}
		case 2: // device state change
			payloadType, topic = "aegis.v1.Device", "raw.device"
			msg = &aegisv1.Device{
				Id:         fmt.Sprintf("dev-%02d", r.Intn(12)),
				PropertyId: PropertyID,
				ZoneId:     fmt.Sprintf("zone-%d", r.Intn(4)),
				State:      aegisv1.DeviceState(1 + r.Intn(4)),
				Attested:   r.Intn(4) != 0,
				Battery:    float32(r.Intn(101)) / 100,
			}
		case 3: // expectation upsert
			payloadType, topic = "aegis.v1.Expectation", "world.delta"
			msg = &aegisv1.Expectation{
				Id:           fmt.Sprintf("exp-%02d", r.Intn(10)),
				PersonId:     fmt.Sprintf("person-%02d", r.Intn(8)),
				ZoneIds:      []string{fmt.Sprintf("zone-%d", r.Intn(4))},
				RegisteredBy: "staff:estate-manager",
			}
		case 4: // alert opened — occasionally later dismissed
			payloadType, topic = "aegis.v1.Alert", "assess.alert"
			st := aegisv1.AlertState_OPEN
			if r.Intn(4) == 0 {
				st = aegisv1.AlertState_DISMISSED
			}
			msg = &aegisv1.Alert{
				Id:          fmt.Sprintf("alert-%02d", r.Intn(16)),
				Severity:    int32(1 + r.Intn(5)),
				ThreatScore: float64(r.Intn(9000)) / 1000,
				EntityId:    fmt.Sprintf("ent-%02d", r.Intn(24)),
				ZoneId:      fmt.Sprintf("zone-%d", r.Intn(4)),
				PropertyId:  PropertyID,
				State:       st,
			}
		case 5: // posture change
			payloadType, topic = "aegis.v1.SetPostureRequest", "world.delta"
			msg = &aegisv1.SetPostureRequest{
				PropertyId: PropertyID,
				Posture:    aegisv1.Posture(1 + r.Intn(5)),
				ActorId:    "operator:console-1",
			}
		case 6: // a kind the M0 fold does not know — must be a counted no-op
			payloadType, topic = "aegis.v1.Sighting", "perception.sighting"
			msg = &aegisv1.Sighting{
				Id:       fmt.Sprintf("sight-%04d", i),
				DeviceId: fmt.Sprintf("dev-%02d", r.Intn(12)),
				Class:    aegisv1.ObjectClass(1 + r.Intn(6)),
			}
		}
		payload, err := proto.MarshalOptions{Deterministic: true}.Marshal(msg)
		if err != nil {
			panic("synthlog: marshal payload: " + err.Error())
		}
		log = append(log, &aegisv1.Envelope{
			EventId:     fmt.Sprintf("evt-%04d", i),
			PropertyId:  PropertyID,
			Topic:       topic,
			PayloadType: payloadType,
			Payload:     payload,
			Prov: &aegisv1.Provenance{
				SourceId:      "sim:synthlog",
				Authority:     aegisv1.Authority_OWNED,
				AuthorityRef:  "consent:owner-ridgeline", // no authority, no ingestion (§3.4)
				CapturedAt:    timestamppb.New(at),
				RecordedAt:    timestamppb.New(at.Add(20 * time.Millisecond)),
				Attested:      true,
				SchemaVersion: "aegis.v1",
			},
		})
	}
	return log
}
