package sync

import (
	"errors"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/diff"
)

func TestAutoDecision(t *testing.T) {
	now := time.Now()

	tests := []struct {
		name string
		s    *diff.Session
		want Decision
	}{
		{
			name: "nil session",
			s:    nil,
			want: DecisionNone,
		},
		{
			name: "error session",
			s:    &diff.Session{Err: errors.New("boom")},
			want: DecisionNone,
		},
		{
			name: "local only",
			s:    &diff.Session{Result: &diff.DiffResult{LocalOnly: true}},
			want: DecisionUpload,
		},
		{
			name: "remote only",
			s:    &diff.Session{Result: &diff.DiffResult{RemoteOnly: true}},
			want: DecisionDownload,
		},
		{
			name: "identical",
			s:    &diff.Session{Result: &diff.DiffResult{}},
			want: DecisionNone,
		},
		{
			name: "local newer",
			s: &diff.Session{Result: &diff.DiffResult{
				Lines:     []diff.DiffLine{{Kind: diff.LineAdded}},
				ModLocal:  now,
				ModRemote: now.Add(-3 * time.Second),
			}},
			want: DecisionUpload,
		},
		{
			name: "remote newer",
			s: &diff.Session{Result: &diff.DiffResult{
				Lines:     []diff.DiffLine{{Kind: diff.LineAdded}},
				ModLocal:  now.Add(-3 * time.Second),
				ModRemote: now,
			}},
			want: DecisionDownload,
		},
		{
			name: "ambiguous mtime",
			s: &diff.Session{Result: &diff.DiffResult{
				Lines:     []diff.DiffLine{{Kind: diff.LineAdded}},
				ModLocal:  now,
				ModRemote: now.Add(-1 * time.Second),
			}},
			want: DecisionNone,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := AutoDecision(tt.s); got != tt.want {
				t.Fatalf("AutoDecision() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestNextDecision(t *testing.T) {
	localOnly := &diff.Session{Result: &diff.DiffResult{LocalOnly: true}}
	remoteOnly := &diff.Session{Result: &diff.DiffResult{RemoteOnly: true}}
	bothSides := &diff.Session{Result: &diff.DiffResult{Lines: []diff.DiffLine{{Kind: diff.LineAdded}}}}
	errorSession := &diff.Session{Err: errors.New("boom"), Result: &diff.DiffResult{LocalOnly: true}}

	if got := NextDecision(DecisionNone, localOnly); got != DecisionUpload {
		t.Fatalf("NextDecision(localOnly, none) = %v, want %v", got, DecisionUpload)
	}
	if got := NextDecision(DecisionUpload, localOnly); got != DecisionDeleteLocal {
		t.Fatalf("NextDecision(localOnly, upload) = %v, want %v", got, DecisionDeleteLocal)
	}
	if got := NextDecision(DecisionDeleteLocal, localOnly); got != DecisionNone {
		t.Fatalf("NextDecision(localOnly, deleteLocal) = %v, want %v", got, DecisionNone)
	}

	if got := NextDecision(DecisionNone, remoteOnly); got != DecisionDownload {
		t.Fatalf("NextDecision(remoteOnly, none) = %v, want %v", got, DecisionDownload)
	}
	if got := NextDecision(DecisionDownload, remoteOnly); got != DecisionDeleteRemote {
		t.Fatalf("NextDecision(remoteOnly, download) = %v, want %v", got, DecisionDeleteRemote)
	}
	if got := NextDecision(DecisionDeleteRemote, remoteOnly); got != DecisionNone {
		t.Fatalf("NextDecision(remoteOnly, deleteRemote) = %v, want %v", got, DecisionNone)
	}

	if got := NextDecision(DecisionNone, bothSides); got != DecisionUpload {
		t.Fatalf("NextDecision(bothSides, none) = %v, want %v", got, DecisionUpload)
	}
	if got := NextDecision(DecisionUpload, bothSides); got != DecisionDownload {
		t.Fatalf("NextDecision(bothSides, upload) = %v, want %v", got, DecisionDownload)
	}
	if got := NextDecision(DecisionDownload, bothSides); got != DecisionNone {
		t.Fatalf("NextDecision(bothSides, download) = %v, want %v", got, DecisionNone)
	}

	if got := NextDecision(DecisionUpload, errorSession); got != DecisionNone {
		t.Fatalf("NextDecision(errorSession, upload) = %v, want %v", got, DecisionNone)
	}
}
