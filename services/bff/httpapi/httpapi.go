// Package httpapi is the bff's M1-dev transport: plain HTTP + SSE over the
// ontology gRPC client. gRPC-web and WebRTC signalling land on top of the
// same route checks later; this package is the shape of the edge, not its
// final wire format.
//
// Doctrine carried over from the parent package: the bff has NO AUTHORITY
// OF ITS OWN. Everything here is a read relay gated by the deny-by-default
// RouteTable. Persona arrives as a header in dev; the production edge
// derives it from mTLS/OIDC — the check below is identical either way.
package httpapi

import (
	"fmt"
	"net/http"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/bff"
	"google.golang.org/protobuf/encoding/protojson"
)

// PersonaHeader carries the caller's persona in the M1-dev transport.
const PersonaHeader = "X-Aegis-Persona"

type Server struct {
	Ontology aegisv1.OntologyServiceClient
	Routes   bff.RouteTable
	Property string
}

func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/twin", s.handleTwin)
	mux.HandleFunc("GET /v1/twin/stream", s.handleTwinStream)
	return mux
}

func personaFrom(r *http.Request) bff.Persona {
	switch r.Header.Get(PersonaHeader) {
	case "OPERATOR":
		return bff.PersonaOperator
	case "PRINCIPAL":
		return bff.PersonaPrincipal
	case "STAFF":
		return bff.PersonaStaff
	case "PARTNER":
		return bff.PersonaPartner
	default:
		return bff.PersonaUnspecified
	}
}

// gate enforces the route table. Unknown persona, unknown method, anything
// not explicitly listed: 403. Fail closed.
func (s *Server) gate(w http.ResponseWriter, r *http.Request, fullMethod string) bool {
	p := personaFrom(r)
	if !s.Routes.Allowed(p, fullMethod) {
		http.Error(w,
			fmt.Sprintf("persona %s may not call %s (deny-by-default)", p, fullMethod),
			http.StatusForbidden)
		return false
	}
	return true
}

func (s *Server) handleTwin(w http.ResponseWriter, r *http.Request) {
	if !s.gate(w, r, "aegis.v1.OntologyService/GetTwin") {
		return
	}
	twin, err := s.Ontology.GetTwin(r.Context(), &aegisv1.GetTwinRequest{PropertyId: s.Property})
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadGateway)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	raw, err := protojson.Marshal(twin)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	_, _ = w.Write(raw)
}

// handleTwinStream relays StreamTwin as server-sent events. Each event's
// data is the protojson of one TwinDelta — first the snapshot, then the
// envelope-per-append projection of the log (ADR-0005).
func (s *Server) handleTwinStream(w http.ResponseWriter, r *http.Request) {
	if !s.gate(w, r, "aegis.v1.OntologyService/StreamTwin") {
		return
	}
	fl, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "streaming unsupported", http.StatusInternalServerError)
		return
	}
	stream, err := s.Ontology.StreamTwin(r.Context(), &aegisv1.StreamTwinRequest{PropertyId: s.Property})
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadGateway)
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	for {
		delta, err := stream.Recv()
		if err != nil {
			return // client gone or upstream closed; SSE just ends
		}
		raw, err := protojson.Marshal(delta)
		if err != nil {
			return
		}
		if _, err := fmt.Fprintf(w, "data: %s\n\n", raw); err != nil {
			return
		}
		fl.Flush()
	}
}
