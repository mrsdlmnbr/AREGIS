// Package bff is the backend-for-frontend (spec §9.13): gRPC-web +
// WebSocket for the console and apps, WebRTC signalling for video, RBAC per
// persona.
//
// DOCTRINE (spec §9.13): the bff has NO AUTHORITY OF ITS OWN. It is a
// projection and a relay. It holds no Governor credential, no asset
// credential, and its route table cannot name GovernorService or
// AssetService at all — the Governor is called only by missions (spec §12),
// assets only by asset-adapter (spec §13). A compromised bff must yield a
// read of the world and some UI verbs, never a moved machine (CLAUDE.md
// rule 1). An operator approves an action via
// MissionService.ApproveAction — the approval carries the operator's
// signature and it is the Governor, behind missions, that authorizes.
//
// M0 scope: the persona enum and the route table. Transport arrives in M1.
package bff

import "strings"

// Persona is the RBAC principal class (spec §1.2, §14).
type Persona int

const (
	PersonaUnspecified Persona = iota
	// PersonaOperator — professional monitoring console (spec §14.1).
	PersonaOperator
	// PersonaPrincipal — the owner and family, owner app (spec §14.2).
	PersonaPrincipal
	// PersonaStaff — estate manager / household staff, staff app (§14.3).
	PersonaStaff
	// PersonaPartner — security detail / law-enforcement liaison; receives
	// HANDOFF material and nothing else.
	PersonaPartner
)

func (p Persona) String() string {
	switch p {
	case PersonaOperator:
		return "OPERATOR"
	case PersonaPrincipal:
		return "PRINCIPAL"
	case PersonaStaff:
		return "STAFF"
	case PersonaPartner:
		return "PARTNER"
	default:
		return "UNSPECIFIED"
	}
}

// RouteTable maps a persona to the full gRPC method names
// ("aegis.v1.Service/Method") the bff will relay for it. Deny-by-default:
// a method not listed does not exist for that persona.
type RouteTable map[Persona][]string

// Allowed reports whether the persona may reach the RPC. Unknown persona or
// unknown RPC → false. Fail closed (CLAUDE.md rule 5).
func (rt RouteTable) Allowed(p Persona, fullMethod string) bool {
	for _, m := range rt[p] {
		if m == fullMethod {
			return true
		}
	}
	return false
}

// forbiddenServices are the services the bff must never relay, for any
// persona, under any configuration. Physical authority flows exclusively
// missions → governor → asset-adapter → asset; the edge never touches it.
// Compiled in — a route file may only remove routes, never add these.
var forbiddenServices = []string{
	"aegis.v1.GovernorService/",
	"aegis.v1.AssetService/",
}

// Validate refuses a route table that names a forbidden service. It is run
// against DefaultRoutes in the tests and must be run against any
// customer-configured table before it is installed.
func Validate(rt RouteTable) []string {
	var violations []string
	for p, methods := range rt {
		for _, m := range methods {
			for _, f := range forbiddenServices {
				if strings.HasPrefix(m, f) {
					violations = append(violations, p.String()+" → "+m)
				}
			}
		}
	}
	return violations
}

// DefaultRoutes is the M0 route table, derived from the interface specs
// (§14.1–14.3) and the decision-rights matrix (§14.4).
func DefaultRoutes() RouteTable {
	return RouteTable{
		// The operator console absorbs the noise: full read of the world,
		// alert handling, and the mission verbs. RequestAction and
		// ApproveAction only *relay to missions* — the signature check and
		// the authorization live behind the Governor, not here.
		PersonaOperator: {
			"aegis.v1.GatewayService/StreamEvents",
			"aegis.v1.GatewayService/ListDevices",
			"aegis.v1.OntologyService/GetTwin",
			"aegis.v1.OntologyService/StreamTwin",
			"aegis.v1.OntologyService/Query",
			"aegis.v1.OntologyService/SetPosture",
			"aegis.v1.ThreatService/StreamAlerts",
			"aegis.v1.ThreatService/Dismiss",
			"aegis.v1.MissionService/StreamMissions",
			"aegis.v1.MissionService/RequestAction",
			"aegis.v1.MissionService/AbortAction", // stopping is always allowed
			"aegis.v1.MissionService/ApproveAction",
			"aegis.v1.EvidenceService/Seal",
			"aegis.v1.EvidenceService/Export",
			"aegis.v1.EvidenceService/Verify",
			"aegis.v1.AgentService/Ask",
			"aegis.v1.AgentService/ProposeCOA", // proposals only; a human acts on them
		},
		// The owner buys silence (spec §14.2): status, posture, their own
		// data — export, revoke. Every access to their data is an
		// AuditRecord they can read (spec §15.6).
		PersonaPrincipal: {
			"aegis.v1.OntologyService/GetTwin",
			"aegis.v1.OntologyService/StreamTwin",
			"aegis.v1.OntologyService/Query",
			"aegis.v1.OntologyService/SetPosture",
			"aegis.v1.OntologyService/RevokeAccess",
			"aegis.v1.ThreatService/StreamAlerts",
			"aegis.v1.EvidenceService/Export",
			"aegis.v1.EvidenceService/Verify",
			"aegis.v1.AgentService/Ask",
		},
		// The staff app is the expectation machine (spec §14.3, §6.2):
		// registering the gardener is the single largest false-alarm
		// reduction in the system.
		PersonaStaff: {
			"aegis.v1.OntologyService/Query",
			"aegis.v1.OntologyService/UpsertPerson",
			"aegis.v1.OntologyService/UpsertExpectation",
			"aegis.v1.OntologyService/GrantAccess",
		},
		// A partner receives sealed evidence and verifies it. Nothing else.
		PersonaPartner: {
			"aegis.v1.EvidenceService/Export",
			"aegis.v1.EvidenceService/Verify",
		},
	}
}
