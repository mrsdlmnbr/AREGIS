package server

import (
	"context"
	"net"
	"testing"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/eventlog"
	"github.com/mrsdlmnbr/aregis/services/ontology/twin"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/test/bufconn"
	"google.golang.org/protobuf/proto"
)

// Fixed clock: time is an input (A7); tests own it completely.
var t0 = time.Date(2026, 3, 14, 3, 11, 40, 0, time.UTC)

func dial(t *testing.T, log eventlog.Log) (aegisv1.OntologyServiceClient, *Server) {
	t.Helper()
	srv, err := New("ridgeline", log, func() time.Time { return t0 })
	if err != nil {
		t.Fatal(err)
	}
	lis := bufconn.Listen(1 << 20)
	gs := grpc.NewServer()
	aegisv1.RegisterOntologyServiceServer(gs, srv)
	go func() { _ = gs.Serve(lis) }()
	t.Cleanup(gs.Stop)
	conn, err := grpc.NewClient("passthrough:///bufnet",
		grpc.WithContextDialer(func(ctx context.Context, _ string) (net.Conn, error) {
			return lis.DialContext(ctx)
		}),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	return aegisv1.NewOntologyServiceClient(conn), srv
}

func TestWritesGoThroughTheLog(t *testing.T) {
	log := eventlog.NewMemLog()
	client, _ := dial(t, log)
	ctx := context.Background()

	if _, err := client.SetPosture(ctx, &aegisv1.SetPostureRequest{
		PropertyId: "ridgeline",
		Posture:    aegisv1.Posture_AWAY,
		ActorId:    "MERIDIAN-2",
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.UpsertExpectation(ctx, &aegisv1.Expectation{
		Id:           "exp-ana-tue",
		PersonId:     "person:ana",
		RegisteredBy: "staff:estate-manager",
	}); err != nil {
		t.Fatal(err)
	}

	// A1: the RPCs appended envelopes; the twin is their fold.
	n, _ := log.Len()
	if n != 2 {
		t.Fatalf("log has %d envelopes, want 2 — a write that skipped the log is a bug", n)
	}
	tw, err := client.GetTwin(ctx, &aegisv1.GetTwinRequest{PropertyId: "ridgeline"})
	if err != nil {
		t.Fatal(err)
	}
	if tw.GetPosture() != aegisv1.Posture_AWAY {
		t.Fatalf("posture = %v", tw.GetPosture())
	}
	if len(tw.GetActiveExpectations()) != 1 {
		t.Fatalf("expectations = %v", tw.GetActiveExpectations())
	}

	// And the projection is REBUILDABLE: fold the log independently and
	// compare canonical bytes (spec §7.2, the weekly CI check in miniature).
	envs, _ := log.ReadFrom(0)
	rebuilt := twin.Rebuild(envs)
	a := twin.MarshalCanonical(rebuilt)
	srv2, err := New("ridgeline", log, func() time.Time { return t0 })
	if err != nil {
		t.Fatal(err)
	}
	srv2.mu.RLock()
	b := twin.MarshalCanonical(srv2.state)
	srv2.mu.RUnlock()
	if string(a) != string(b) {
		t.Fatal("rebuild produced different bytes than the serving fold")
	}
}

func TestStreamTwinIsAProjectionOfTheLog(t *testing.T) {
	log := eventlog.NewMemLog()
	client, _ := dial(t, log)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	stream, err := client.StreamTwin(ctx, &aegisv1.StreamTwinRequest{PropertyId: "ridgeline"})
	if err != nil {
		t.Fatal(err)
	}
	// First delta: the full twin snapshot.
	first, err := stream.Recv()
	if err != nil {
		t.Fatal(err)
	}
	snap := &aegisv1.Twin{}
	if err := proto.Unmarshal(first.GetPatch(), snap); err != nil {
		t.Fatalf("first delta is not a Twin snapshot: %v", err)
	}

	// A write arrives → the stream carries the ENVELOPE that changed the twin.
	if _, err := client.SetPosture(ctx, &aegisv1.SetPostureRequest{
		PropertyId: "ridgeline", Posture: aegisv1.Posture_NIGHT, ActorId: "MERIDIAN-2",
	}); err != nil {
		t.Fatal(err)
	}
	delta, err := stream.Recv()
	if err != nil {
		t.Fatal(err)
	}
	env := &aegisv1.Envelope{}
	if err := proto.Unmarshal(delta.GetPatch(), env); err != nil {
		t.Fatalf("delta is not an Envelope: %v", err)
	}
	if env.GetPayloadType() != twin.PayloadPosture {
		t.Fatalf("delta payload = %s", env.GetPayloadType())
	}
	if env.GetProv().GetAuthorityRef() == "" {
		t.Fatal("even admin events carry provenance (A5)")
	}
}

func TestRefusals(t *testing.T) {
	log := eventlog.NewMemLog()
	client, _ := dial(t, log)
	ctx := context.Background()

	// Posture change with no actor: invisible actions do not exist here.
	if _, err := client.SetPosture(ctx, &aegisv1.SetPostureRequest{
		PropertyId: "ridgeline", Posture: aegisv1.Posture_LOCKDOWN,
	}); err == nil {
		t.Fatal("actor-less posture change must be refused")
	}
	// Biometric template without consent: refused at the API (§3.5).
	if _, err := client.UpsertPerson(ctx, &aegisv1.Person{
		Id: "person:x", EmbeddingIds: []string{"emb-1"}, ConsentBiometric: false,
	}); err == nil {
		t.Fatal("unconsented biometrics must be refused")
	}
	// Unknown structured query: closed surface.
	if _, err := client.Query(ctx, &aegisv1.QueryRequest{
		PropertyId: "ridgeline", Query: "DROP TABLE alert",
	}); err == nil {
		t.Fatal("the query surface is closed")
	}
	// And none of the refusals touched the log.
	if n, _ := log.Len(); n != 0 {
		t.Fatalf("refused writes appended %d envelopes", n)
	}
}

func TestRestartIsARebuild(t *testing.T) {
	log := eventlog.NewMemLog()
	client, _ := dial(t, log)
	ctx := context.Background()
	if _, err := client.SetPosture(ctx, &aegisv1.SetPostureRequest{
		PropertyId: "ridgeline", Posture: aegisv1.Posture_ELEVATED, ActorId: "op",
	}); err != nil {
		t.Fatal(err)
	}
	// "Restart": a fresh server over the same log sees the same world.
	fresh, err := New("ridgeline", log, func() time.Time { return t0 })
	if err != nil {
		t.Fatal(err)
	}
	fresh.mu.RLock()
	posture := fresh.state.Posture
	fresh.mu.RUnlock()
	if posture != aegisv1.Posture_ELEVATED {
		t.Fatalf("restarted fold lost the posture: %v", posture)
	}
}
