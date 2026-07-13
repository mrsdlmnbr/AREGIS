package sync

import (
	"errors"
	"testing"
)

// M0: every job is a typed stub. When a job gains a real implementation,
// its line here is deleted in the same commit — this test is the tripwire
// against a half-implemented job silently "succeeding".
func TestAllJobsReturnNotImplemented(t *testing.T) {
	jobs := []interface{ Run() error }{
		FederateUnknownHandlesJob{},
		EncryptedBackupJob{},
		SignedOTAJob{},
	}
	for _, j := range jobs {
		if err := j.Run(); !errors.Is(err, ErrNotImplementedInM0) {
			t.Fatalf("%T.Run() = %v, want ErrNotImplementedInM0", j, err)
		}
	}
}
