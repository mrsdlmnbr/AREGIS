// Package server serves the digital twin over gRPC (spec §9.4).
//
// The A1 discipline, in code: every WRITE RPC appends an envelope to the
// event log and then folds its OWN append into the in-memory twin. The twin
// is never mutated directly — it is always the fold of the log, so dropping
// it and calling Rebuild produces the same bytes (rebuild-check proves it).
package server

import (
	"context"
	"fmt"
	"sort"
	"sync"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/eventlog"
	"github.com/mrsdlmnbr/aregis/services/ontology/twin"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/timestamppb"
)

type Server struct {
	aegisv1.UnimplementedOntologyServiceServer

	propertyID string
	log        eventlog.Log
	// Time is an input (A7): injected at construction. The ontologyd edge
	// passes the wall clock; tests pass a fixed one.
	now func() time.Time

	mu    sync.RWMutex
	state *twin.Twin
	seq   uint64
}

// New folds the existing log and serves from there — a restart is a rebuild,
// which is the whole point (spec §7.2).
func New(propertyID string, log eventlog.Log, now func() time.Time) (*Server, error) {
	envs, err := log.ReadFrom(0)
	if err != nil {
		return nil, err
	}
	n, err := log.Len()
	if err != nil {
		return nil, err
	}
	return &Server{
		propertyID: propertyID,
		log:        log,
		now:        now,
		state:      twin.Rebuild(envs),
		seq:        n,
	}, nil
}

// append is the single write path: log first, fold second, ack third.
func (s *Server) append(payloadType string, msg proto.Message) error {
	payload, err := proto.MarshalOptions{Deterministic: true}.Marshal(msg)
	if err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.seq++
	at := timestamppb.New(s.now())
	env := &aegisv1.Envelope{
		EventId:     fmt.Sprintf("adm-%06d", s.seq),
		PropertyId:  s.propertyID,
		Topic:       "world.delta",
		PayloadType: payloadType,
		Payload:     payload,
		Prov: &aegisv1.Provenance{
			SourceId:      "ontology",
			Authority:     aegisv1.Authority_OWNED,
			AuthorityRef:  "estate-owner-" + s.propertyID,
			CapturedAt:    at,
			RecordedAt:    at,
			SchemaVersion: "aegis.v1",
		},
	}
	if _, err := s.log.Append(env); err != nil {
		return err
	}
	twin.Apply(s.state, env)
	return nil
}

func (s *Server) GetTwin(_ context.Context, req *aegisv1.GetTwinRequest) (*aegisv1.Twin, error) {
	if req.GetPropertyId() != s.propertyID {
		return nil, status.Errorf(codes.NotFound, "unknown property %q", req.GetPropertyId())
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.state.Proto(), nil
}

// StreamTwin sends the full twin as the first delta, then one delta per
// subsequent log append. The patch bytes ARE the envelope that changed the
// twin: the stream is visibly a projection of the log, exactly like the
// console's watch tape (spec §14.1).
func (s *Server) StreamTwin(req *aegisv1.StreamTwinRequest, stream aegisv1.OntologyService_StreamTwinServer) error {
	if req.GetPropertyId() != s.propertyID {
		return status.Errorf(codes.NotFound, "unknown property %q", req.GetPropertyId())
	}
	s.mu.RLock()
	snapshot, err := proto.MarshalOptions{Deterministic: true}.Marshal(s.state.Proto())
	s.mu.RUnlock()
	if err != nil {
		return err
	}
	if err := stream.Send(&aegisv1.TwinDelta{PropertyId: s.propertyID, Patch: snapshot}); err != nil {
		return err
	}
	ch := make(chan *aegisv1.Envelope, 256)
	cancel := s.log.Subscribe(ch)
	defer cancel()
	for {
		select {
		case <-stream.Context().Done():
			return nil
		case env := <-ch:
			raw, err := proto.MarshalOptions{Deterministic: true}.Marshal(env)
			if err != nil {
				return err
			}
			if err := stream.Send(&aegisv1.TwinDelta{PropertyId: s.propertyID, Patch: raw}); err != nil {
				return err
			}
		}
	}
}

// Query: structured, read-only (spec §9.4 — never SQL from the outside).
// M1 supports the closed set the console and agent need.
func (s *Server) Query(_ context.Context, req *aegisv1.QueryRequest) (*aegisv1.QueryResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	var result string
	switch req.GetQuery() {
	case "posture":
		result = fmt.Sprintf(`{"posture":%q}`, s.state.Posture.String())
	case "open_alert_count":
		result = fmt.Sprintf(`{"open_alerts":%d}`, len(s.state.OpenAlerts))
	case "entity_ids":
		ids := make([]string, 0, len(s.state.Entities))
		for id := range s.state.Entities {
			ids = append(ids, id)
		}
		sort.Strings(ids)
		result = fmt.Sprintf(`{"entity_ids":%q}`, ids)
	default:
		return nil, status.Errorf(codes.InvalidArgument,
			"unknown query %q — the query surface is closed and read-only", req.GetQuery())
	}
	return &aegisv1.QueryResponse{ResultJson: []byte(result)}, nil
}

func (s *Server) UpsertExpectation(_ context.Context, x *aegisv1.Expectation) (*aegisv1.Ack, error) {
	if x.GetId() == "" {
		return nil, status.Error(codes.InvalidArgument, "expectation needs an id")
	}
	if err := s.append(twin.PayloadExpectation, x); err != nil {
		return nil, err
	}
	return &aegisv1.Ack{Ok: true}, nil
}

func (s *Server) SetPosture(_ context.Context, req *aegisv1.SetPostureRequest) (*aegisv1.Ack, error) {
	if req.GetActorId() == "" {
		// Nothing this system does may be invisible: a posture change
		// without an actor is refused, not defaulted (CLAUDE.md done-rule 5).
		return nil, status.Error(codes.InvalidArgument, "posture change requires actor_id")
	}
	if err := s.append(twin.PayloadPosture, req); err != nil {
		return nil, err
	}
	return &aegisv1.Ack{Ok: true}, nil
}

func (s *Server) UpsertPerson(_ context.Context, p *aegisv1.Person) (*aegisv1.Ack, error) {
	if p.GetId() == "" {
		return nil, status.Error(codes.InvalidArgument, "person needs an id")
	}
	if len(p.GetEmbeddingIds()) > 0 && !p.GetConsentBiometric() {
		// §3.5: no consent, no template. Refused at the API, not filtered later.
		return nil, status.Error(codes.FailedPrecondition,
			"biometric templates require consent_biometric=true (spec §3.5)")
	}
	if err := s.append("aegis.v1.Person", p); err != nil {
		return nil, err
	}
	return &aegisv1.Ack{Ok: true}, nil
}
