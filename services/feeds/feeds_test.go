package feeds

import (
	"strings"
	"testing"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
)

func meshConnector() Connector {
	return Connector{
		ID:           "mesh-north",
		Kind:         "mesh",
		Authority:    aegisv1.Authority_MESH_CONSENTED,
		AuthorityRef: "consent:mesh-agreement-0042",
	}
}

// No authority_ref, no ingestion (spec §3.4). The refusal is structural.
func TestStartRefusesEmptyAuthorityRef(t *testing.T) {
	r := NewRegistry()
	c := meshConnector()
	c.AuthorityRef = ""
	err := r.Start(c)
	if err == nil {
		t.Fatal("connector with empty authority_ref started; must be refused")
	}
	if !strings.Contains(err.Error(), "no authority, no ingestion") {
		t.Fatalf("refusal must cite the doctrine, got: %v", err)
	}
	if e := r.RecordDerivedEvent(c.ID, "evt-1"); e == nil {
		t.Fatal("refused connector accepted a derived event")
	}
}

func TestStartRefusesUnspecifiedAuthority(t *testing.T) {
	r := NewRegistry()
	c := meshConnector()
	c.Authority = aegisv1.Authority_AUTHORITY_UNSPECIFIED
	if err := r.Start(c); err == nil {
		t.Fatal("connector with AUTHORITY_UNSPECIFIED started; must be refused")
	}
}

func TestRevocationMarksDerivedEvents(t *testing.T) {
	r := NewRegistry()
	c := meshConnector()
	if err := r.Start(c); err != nil {
		t.Fatalf("start: %v", err)
	}
	for _, id := range []string{"evt-3", "evt-1", "evt-2"} {
		if err := r.RecordDerivedEvent(c.ID, id); err != nil {
			t.Fatalf("record %s: %v", id, err)
		}
	}

	// Time is an input (A7): the withdrawal carries its own timestamp.
	at := time.Date(2026, time.March, 14, 3, 11, 45, 0, time.UTC)
	ids, err := r.Revoke(c.ID, at)
	if err != nil {
		t.Fatalf("revoke: %v", err)
	}
	want := []string{"evt-1", "evt-2", "evt-3"}
	if len(ids) != len(want) {
		t.Fatalf("derived ids = %v, want %v", ids, want)
	}
	for i := range want {
		if ids[i] != want[i] {
			t.Fatalf("derived ids = %v, want %v (sorted, deterministic)", ids, want)
		}
	}

	revoked, revokedAt := r.Revoked(c.ID)
	if !revoked || !revokedAt.Equal(at) {
		t.Fatalf("Revoked() = %v @ %v, want true @ %v", revoked, revokedAt, at)
	}

	// After revocation: no further ingestion, no silent restart.
	if err := r.RecordDerivedEvent(c.ID, "evt-4"); err == nil {
		t.Fatal("revoked connector accepted a derived event")
	}
	if err := r.Start(c); err == nil {
		t.Fatal("revoked connector restarted without re-consent")
	}
	if _, err := r.Revoke(c.ID, at.Add(time.Minute)); err == nil {
		t.Fatal("double revoke must be an error, not a silent success")
	}
}

func TestRevokeUnknownConnector(t *testing.T) {
	r := NewRegistry()
	if _, err := r.Revoke("ghost", time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)); err == nil {
		t.Fatal("revoking an unregistered connector must error")
	}
}

// Spec §9.10 acceptance: derived data is out of the twin within 60 s of
// revocation. The deadline arithmetic is compiled in.
func TestRevocationDeadline(t *testing.T) {
	at := time.Date(2026, time.March, 14, 3, 11, 45, 500_000_000, time.UTC)
	want := time.Date(2026, time.March, 14, 3, 12, 45, 500_000_000, time.UTC)
	if got := RevocationDeadline(at); !got.Equal(want) {
		t.Fatalf("RevocationDeadline(%v) = %v, want %v", at, got, want)
	}
	if RevocationSLA != 60*time.Second {
		t.Fatalf("RevocationSLA = %v, want 60s (spec §9.10)", RevocationSLA)
	}
}
