package eventlog

import (
	"os"
	"path/filepath"
	"testing"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"google.golang.org/protobuf/proto"
)

func env(id string) *aegisv1.Envelope {
	return &aegisv1.Envelope{
		EventId:     id,
		PropertyId:  "ridgeline",
		Topic:       "world.delta",
		PayloadType: "test",
		Payload:     []byte(id),
	}
}

func TestMemLogAppendRead(t *testing.T) {
	l := NewMemLog()
	for _, id := range []string{"a", "b", "c"} {
		if _, err := l.Append(env(id)); err != nil {
			t.Fatal(err)
		}
	}
	got, err := l.ReadFrom(1)
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 2 || got[0].GetEventId() != "b" {
		t.Fatalf("ReadFrom(1) = %v", got)
	}
}

func TestSubscribeSeesNewAppendsOnly(t *testing.T) {
	l := NewMemLog()
	_, _ = l.Append(env("before"))
	ch := make(chan *aegisv1.Envelope, 4)
	cancel := l.Subscribe(ch)
	defer cancel()
	_, _ = l.Append(env("after"))
	select {
	case e := <-ch:
		if e.GetEventId() != "after" {
			t.Fatalf("got %s", e.GetEventId())
		}
	default:
		t.Fatal("subscriber saw nothing")
	}
}

func TestSlowSubscriberIsDroppedNotBlocking(t *testing.T) {
	l := NewMemLog()
	ch := make(chan *aegisv1.Envelope) // unbuffered, never serviced
	cancel := l.Subscribe(ch)
	defer cancel()
	// Must not deadlock: the log outranks any reader.
	if _, err := l.Append(env("x")); err != nil {
		t.Fatal(err)
	}
}

func TestFileLogSurvivesReopen(t *testing.T) {
	path := filepath.Join(t.TempDir(), "aegis.log")
	l, err := OpenFileLog(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, id := range []string{"a", "b"} {
		if _, err := l.Append(env(id)); err != nil {
			t.Fatal(err)
		}
	}
	_ = l.Close()

	re, err := OpenFileLog(path)
	if err != nil {
		t.Fatal(err)
	}
	defer re.Close()
	got, _ := re.ReadFrom(0)
	if len(got) != 2 || !proto.Equal(got[0], env("a")) {
		t.Fatalf("reopened log = %v", got)
	}
	// And it keeps appending after the reopen.
	if _, err := re.Append(env("c")); err != nil {
		t.Fatal(err)
	}
	n, _ := re.Len()
	if n != 3 {
		t.Fatalf("len = %d", n)
	}
}

func TestCorruptLogRefusesToServe(t *testing.T) {
	path := filepath.Join(t.TempDir(), "aegis.log")
	if err := os.WriteFile(path, []byte("!!!not-base64!!!\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := OpenFileLog(path); err == nil {
		t.Fatal("a corrupt log must refuse to serve, not skip lines (I7 posture)")
	}
}
