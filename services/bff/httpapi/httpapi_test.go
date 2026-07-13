package httpapi

import (
	"bufio"
	"context"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/bff"
	"github.com/mrsdlmnbr/aregis/services/eventlog"
	ontoserver "github.com/mrsdlmnbr/aregis/services/ontology/server"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/test/bufconn"
)

var t0 = time.Date(2026, 3, 14, 3, 11, 40, 0, time.UTC)

// A real edge: bff HTTP → gRPC (bufconn) → ontology server → event log.
func stack(t *testing.T) (*httptest.Server, aegisv1.OntologyServiceClient) {
	t.Helper()
	log := eventlog.NewMemLog()
	onto, err := ontoserver.New("ridgeline", log, func() time.Time { return t0 })
	if err != nil {
		t.Fatal(err)
	}
	lis := bufconn.Listen(1 << 20)
	gs := grpc.NewServer()
	aegisv1.RegisterOntologyServiceServer(gs, onto)
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
	client := aegisv1.NewOntologyServiceClient(conn)
	srv := &Server{Ontology: client, Routes: bff.DefaultRoutes(), Property: "ridgeline"}
	ts := httptest.NewServer(srv.Handler())
	t.Cleanup(ts.Close)
	return ts, client
}

func get(t *testing.T, ts *httptest.Server, path, persona string) *http.Response {
	t.Helper()
	req, _ := http.NewRequest(http.MethodGet, ts.URL+path, nil)
	if persona != "" {
		req.Header.Set(PersonaHeader, persona)
	}
	resp, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	return resp
}

func TestOperatorReadsTheTwin(t *testing.T) {
	ts, _ := stack(t)
	resp := get(t, ts, "/v1/twin", "OPERATOR")
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status %d", resp.StatusCode)
	}
	if ct := resp.Header.Get("Content-Type"); ct != "application/json" {
		t.Fatalf("content-type %s", ct)
	}
}

func TestDenyByDefault(t *testing.T) {
	ts, _ := stack(t)
	for _, tc := range []struct {
		path, persona string
	}{
		{"/v1/twin", ""},             // no persona header
		{"/v1/twin", "PARTNER"},      // partner gets HANDOFF material only
		{"/v1/twin", "JANITOR"},      // unknown persona string
		{"/v1/twin", "STAFF"},        // staff sees their zones via Query, not the twin
		{"/v1/twin/stream", "STAFF"}, // and never the live stream
		{"/v1/twin/stream", "PARTNER"},
	} {
		resp := get(t, ts, tc.path, tc.persona)
		resp.Body.Close()
		if resp.StatusCode != http.StatusForbidden {
			t.Errorf("%s as %q: status %d, want 403", tc.path, tc.persona, resp.StatusCode)
		}
	}
}

func TestStreamRelaysSnapshotThenDeltas(t *testing.T) {
	ts, client := stack(t)
	req, _ := http.NewRequest(http.MethodGet, ts.URL+"/v1/twin/stream", nil)
	req.Header.Set(PersonaHeader, "OPERATOR")
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	resp, err := ts.Client().Do(req.WithContext(ctx))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status %d", resp.StatusCode)
	}

	sc := bufio.NewScanner(resp.Body)
	readEvent := func() string {
		for sc.Scan() {
			line := sc.Text()
			if strings.HasPrefix(line, "data: ") {
				return strings.TrimPrefix(line, "data: ")
			}
		}
		t.Fatal("stream ended early")
		return ""
	}

	// First event: the full-twin snapshot. An EMPTY twin marshals to zero
	// bytes, so protojson rightly omits `patch` — the property id is the
	// delta's identity either way.
	first := readEvent()
	if !strings.Contains(first, `"propertyId":"ridgeline"`) {
		t.Fatalf("first SSE event is not a TwinDelta: %s", first)
	}

	// A write through the ontology API shows up as the next delta, carrying
	// the envelope that changed the twin (ADR-0005) — non-empty patch.
	if _, err := client.SetPosture(context.Background(), &aegisv1.SetPostureRequest{
		PropertyId: "ridgeline", Posture: aegisv1.Posture_AWAY, ActorId: "MERIDIAN-2",
	}); err != nil {
		t.Fatal(err)
	}
	second := readEvent()
	if !strings.Contains(second, `"patch"`) {
		t.Fatalf("second SSE event carries no envelope patch: %s", second)
	}
}
