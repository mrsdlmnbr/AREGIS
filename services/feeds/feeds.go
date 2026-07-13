// Package feeds is external federation with authority enforcement
// (spec §9.10, §3.4, axiom A5: provenance on every byte).
//
// DOCTRINE (spec §3.4): every connector declares its Authority and an
// authority_ref — a consent record, a mesh agreement, a licence. A connector
// with no valid authority ref CANNOT START. Accessible is not authorized;
// the discipline is structural, not optional. There is no override flag and
// none may be added.
//
// Feed content is untrusted input (spec §15.5): nothing in this package is,
// or may ever become, a path to any component that can act.
//
// Time is an input (axiom A7): revocation timestamps arrive as parameters,
// never from the wall clock.
package feeds

import (
	"fmt"
	"sort"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
)

// RevocationSLA is the spec §9.10 acceptance bound: revoking a consent
// removes that node's data from the twin within 60 seconds. Compiled in,
// not config — a policy may tighten it, never loosen it.
const RevocationSLA = 60 * time.Second

// Connector is one external feed source: neighbour mesh, HOA, weather,
// wildfire, ADS-B, AIS, public dispatch, threat-intel (spec §9.10).
type Connector struct {
	ID   string
	Kind string // "mesh" | "hoa" | "weather" | "wildfire" | "adsb" | "ais" | "dispatch" | "threatintel"
	// Authority is the tier under which this feed's data is collected
	// (spec §3.4). AUTHORITY_UNSPECIFIED is not a tier — it is a refusal.
	Authority aegisv1.Authority
	// AuthorityRef names the consent record, mesh agreement, or licence.
	// EMPTY => the connector must not start. No authority, no ingestion.
	AuthorityRef string
}

// connectorState tracks a started connector and the events derived from it,
// so that a revocation can name every derived event to be marked
// authority_revoked — without deleting anything (the audit record stays).
type connectorState struct {
	connector Connector
	revoked   bool
	revokedAt time.Time
	// derivedEvents is the set of event IDs this connector's data produced.
	derivedEvents map[string]struct{}
}

// Registry holds started connectors and the in-memory eventID→connectorID
// index needed to honour revocation (spec §9.10 acceptance).
type Registry struct {
	connectors map[string]*connectorState
	// eventOwner indexes derived event ID → connector ID.
	eventOwner map[string]string
}

func NewRegistry() *Registry {
	return &Registry{
		connectors: map[string]*connectorState{},
		eventOwner: map[string]string{},
	}
}

// Start admits a connector, or refuses it. The two refusals below are the
// load-bearing lines of this service (spec §3.4): the temptation to quietly
// tap "just one more feed" must fail structurally, not procedurally.
func (r *Registry) Start(c Connector) error {
	if c.ID == "" {
		return fmt.Errorf("refusing to start connector: empty connector id")
	}
	if c.AuthorityRef == "" {
		return fmt.Errorf(
			"refusing to start connector %q (%s): empty authority_ref — no authority, no ingestion (spec §3.4)",
			c.ID, c.Kind)
	}
	if c.Authority == aegisv1.Authority_AUTHORITY_UNSPECIFIED {
		return fmt.Errorf(
			"refusing to start connector %q (%s): authority UNSPECIFIED — accessible is not authorized (spec §3.4)",
			c.ID, c.Kind)
	}
	if existing, ok := r.connectors[c.ID]; ok {
		if existing.revoked {
			// A revoked consent does not come back by restarting the
			// connector; re-admission needs a NEW authority record.
			return fmt.Errorf(
				"refusing to start connector %q: authority %q was revoked — re-consent required (spec §9.10)",
				c.ID, existing.connector.AuthorityRef)
		}
		return fmt.Errorf("connector %q already started", c.ID)
	}
	r.connectors[c.ID] = &connectorState{
		connector:     c,
		derivedEvents: map[string]struct{}{},
	}
	return nil
}

// RecordDerivedEvent indexes an event derived from a connector's data, so a
// later revocation can name it. Ingestion for an unknown or revoked
// connector is refused — data may only enter under a live authority.
func (r *Registry) RecordDerivedEvent(connectorID, eventID string) error {
	st, ok := r.connectors[connectorID]
	if !ok {
		return fmt.Errorf("refusing event %q: connector %q not started — no authority, no ingestion (spec §3.4)", eventID, connectorID)
	}
	if st.revoked {
		return fmt.Errorf("refusing event %q: connector %q authority revoked (spec §9.10)", eventID, connectorID)
	}
	st.derivedEvents[eventID] = struct{}{}
	r.eventOwner[eventID] = connectorID
	return nil
}

// Revoke marks the connector's authority revoked as of `at` (time is an
// input — the consent withdrawal carries its own timestamp) and returns the
// sorted list of derived event IDs to be marked authority_revoked in the
// twin and alert projections. Nothing is deleted: the audit record of what
// was ingested, and under what authority, outlives the authority itself
// (spec §9.10 acceptance).
func (r *Registry) Revoke(connectorID string, at time.Time) ([]string, error) {
	st, ok := r.connectors[connectorID]
	if !ok {
		return nil, fmt.Errorf("cannot revoke connector %q: not registered", connectorID)
	}
	if st.revoked {
		return nil, fmt.Errorf("connector %q already revoked at %s", connectorID, st.revokedAt.UTC().Format(time.RFC3339))
	}
	st.revoked = true
	st.revokedAt = at

	ids := make([]string, 0, len(st.derivedEvents))
	for id := range st.derivedEvents {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids, nil
}

// Revoked reports whether a connector's authority has been revoked, and when.
func (r *Registry) Revoked(connectorID string) (bool, time.Time) {
	st, ok := r.connectors[connectorID]
	if !ok || !st.revoked {
		return false, time.Time{}
	}
	return true, st.revokedAt
}

// RevocationDeadline is the moment by which every derived datum must be out
// of the twin and every derived alert marked authority_revoked:
// revokedAt + 60 s (spec §9.10 acceptance).
func RevocationDeadline(revokedAt time.Time) time.Time {
	return revokedAt.Add(RevocationSLA)
}
