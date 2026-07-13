package bff

import "testing"

// The load-bearing test of this package (spec §9.13, CLAUDE.md rule 1): no
// persona, ever, routes to the Governor or to direct asset tasking through
// the bff. The bff has no authority of its own.
func TestNoPersonaRoutesToGovernorOrAssetTask(t *testing.T) {
	rt := DefaultRoutes()
	forbidden := []string{
		"aegis.v1.GovernorService/Authorize",
		"aegis.v1.AssetService/Task",
	}
	for _, p := range []Persona{PersonaOperator, PersonaPrincipal, PersonaStaff, PersonaPartner, PersonaUnspecified} {
		for _, f := range forbidden {
			if rt.Allowed(p, f) {
				t.Fatalf("persona %s routes to %s — the bff must have no authority of its own (spec §9.13)", p, f)
			}
		}
	}
	// Stronger: the whole GovernorService and AssetService namespaces are
	// forbidden, not just the two RPCs above.
	if v := Validate(rt); len(v) != 0 {
		t.Fatalf("route table names a forbidden service: %v", v)
	}
}

func TestValidateCatchesForbiddenRoute(t *testing.T) {
	rt := RouteTable{
		PersonaOperator: {"aegis.v1.GovernorService/Authorize"},
	}
	if v := Validate(rt); len(v) != 1 {
		t.Fatalf("Validate missed a GovernorService route: %v", v)
	}
}

func TestAllowedIsDenyByDefault(t *testing.T) {
	rt := DefaultRoutes()
	if rt.Allowed(PersonaUnspecified, "aegis.v1.OntologyService/GetTwin") {
		t.Fatal("unspecified persona has routes; must be deny-by-default")
	}
	if rt.Allowed(PersonaPartner, "aegis.v1.OntologyService/GetTwin") {
		t.Fatal("partner can read the twin; partner receives sealed evidence only")
	}
	if !rt.Allowed(PersonaOperator, "aegis.v1.ThreatService/StreamAlerts") {
		t.Fatal("operator cannot stream alerts; the console is unusable")
	}
}
