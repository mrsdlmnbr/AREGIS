// Package eventlog is the appliance's event log seam (axiom A1: the log is
// the truth; every store is a rebuildable projection of it).
//
// M0/M1-dev backends: MemLog and FileLog. The production backend is
// Redpanda (spec §7.1) behind this same interface, wired in deploy/compose;
// nothing above this seam may know which backend it is on, because black-
// start recovery (§16) depends on being able to fold ANY faithful copy of
// the log.
//
// FileLog encoding: one line per envelope, base64(deterministic proto
// marshal). Deterministic marshaling matters: `make rebuild-from-log` and
// the weekly CI rebuild byte-compare projections, and a non-canonical
// encoding would make equal logs look different.
package eventlog

import (
	"bufio"
	"encoding/base64"
	"fmt"
	"os"
	"sync"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"google.golang.org/protobuf/proto"
)

// Log is an append-only, offset-addressed sequence of envelopes.
type Log interface {
	// Append adds one envelope and returns its offset. Envelopes are
	// immutable once appended — there is no update and no delete, ever.
	Append(env *aegisv1.Envelope) (uint64, error)
	// ReadFrom returns all envelopes at offset >= from, in order.
	ReadFrom(from uint64) ([]*aegisv1.Envelope, error)
	// Len returns the next offset (== number of envelopes).
	Len() (uint64, error)
	// Subscribe registers a channel that receives every envelope appended
	// AFTER the call. The channel must be serviced; a full channel drops
	// the subscriber, never blocks the log (backpressure: the log outranks
	// any reader).
	Subscribe(ch chan<- *aegisv1.Envelope) (cancel func())
}

func marshalDet(env *aegisv1.Envelope) ([]byte, error) {
	return proto.MarshalOptions{Deterministic: true}.Marshal(env)
}

// ── MemLog ──────────────────────────────────────────────────────────────────

type MemLog struct {
	mu   sync.RWMutex
	envs []*aegisv1.Envelope
	subs map[int]chan<- *aegisv1.Envelope
	next int
}

func NewMemLog() *MemLog {
	return &MemLog{subs: map[int]chan<- *aegisv1.Envelope{}}
}

func (l *MemLog) Append(env *aegisv1.Envelope) (uint64, error) {
	l.mu.Lock()
	l.envs = append(l.envs, proto.Clone(env).(*aegisv1.Envelope))
	off := uint64(len(l.envs) - 1)
	subs := make([]chan<- *aegisv1.Envelope, 0, len(l.subs))
	dead := []int{}
	for id, ch := range l.subs {
		select {
		case ch <- l.envs[off]:
			subs = append(subs, ch)
		default:
			dead = append(dead, id) // never block the log
		}
	}
	for _, id := range dead {
		delete(l.subs, id)
	}
	l.mu.Unlock()
	_ = subs
	return off, nil
}

func (l *MemLog) ReadFrom(from uint64) ([]*aegisv1.Envelope, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()
	if from > uint64(len(l.envs)) {
		return nil, nil
	}
	out := make([]*aegisv1.Envelope, 0, uint64(len(l.envs))-from)
	for _, e := range l.envs[from:] {
		out = append(out, proto.Clone(e).(*aegisv1.Envelope))
	}
	return out, nil
}

func (l *MemLog) Len() (uint64, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()
	return uint64(len(l.envs)), nil
}

func (l *MemLog) Subscribe(ch chan<- *aegisv1.Envelope) func() {
	l.mu.Lock()
	id := l.next
	l.next++
	l.subs[id] = ch
	l.mu.Unlock()
	return func() {
		l.mu.Lock()
		delete(l.subs, id)
		l.mu.Unlock()
	}
}

// ── FileLog ─────────────────────────────────────────────────────────────────

// FileLog persists the log as append-only lines. It wraps a MemLog for
// serving and fans every append through to disk before acknowledging —
// an unacknowledged append is an append that did not happen.
type FileLog struct {
	mem  *MemLog
	mu   sync.Mutex
	file *os.File
}

func OpenFileLog(path string) (*FileLog, error) {
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR|os.O_APPEND, 0o644)
	if err != nil {
		return nil, err
	}
	l := &FileLog{mem: NewMemLog(), file: f}
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 1<<20), 1<<24)
	line := 0
	for sc.Scan() {
		line++
		raw, err := base64.StdEncoding.DecodeString(sc.Text())
		if err != nil {
			return nil, fmt.Errorf("log line %d: not base64 — the log is corrupt, refusing to serve (I7 posture): %w", line, err)
		}
		env := &aegisv1.Envelope{}
		if err := proto.Unmarshal(raw, env); err != nil {
			return nil, fmt.Errorf("log line %d: not an Envelope — refusing to serve: %w", line, err)
		}
		if _, err := l.mem.Append(env); err != nil {
			return nil, err
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	return l, nil
}

func (l *FileLog) Append(env *aegisv1.Envelope) (uint64, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	raw, err := marshalDet(env)
	if err != nil {
		return 0, err
	}
	if _, err := l.file.WriteString(base64.StdEncoding.EncodeToString(raw) + "\n"); err != nil {
		return 0, err
	}
	if err := l.file.Sync(); err != nil {
		return 0, err
	}
	return l.mem.Append(env)
}

func (l *FileLog) ReadFrom(from uint64) ([]*aegisv1.Envelope, error) { return l.mem.ReadFrom(from) }
func (l *FileLog) Len() (uint64, error)                              { return l.mem.Len() }
func (l *FileLog) Subscribe(ch chan<- *aegisv1.Envelope) func()      { return l.mem.Subscribe(ch) }
func (l *FileLog) Close() error                                      { return l.file.Close() }
