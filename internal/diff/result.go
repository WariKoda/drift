// Package diff compares local and remote files and produces structured diff results.
package diff

import "time"

// LineKind classifies a line in a unified diff.
type LineKind int

const (
	LineEqual LineKind = iota
	LineAdded          // present on remote, absent on local
	LineRemoved        // present on local, absent on remote
)

// DiffLine holds one line of a unified diff sequence.
type DiffLine struct {
	Text      string
	Kind      LineKind // Equal, Added, Removed
	LocalNum  int      // 0 = this side has no number
	RemoteNum int
}

// DiffResult is the comparison output for a single file pair.
type DiffResult struct {
	LocalPath  string
	RemotePath string
	Lines      []DiffLine

	// Existence flags
	LocalOnly  bool // file exists only locally (not on remote)
	RemoteOnly bool // file exists only on remote

	// Metadata
	Binary      bool
	ContentDiff bool // content differs but has no renderable line diff
	SizeLocal   int64
	SizeRemote  int64
	ModLocal    time.Time
	ModRemote   time.Time

	// Cached statistics over Lines. Lines is never mutated after Compare
	// builds it, so these are computed once (lazily) and reused. Scanning the
	// line slice on every frame/query was a measurable cost for large files.
	statsReady bool
	differs    bool
	added      int
	removed    int
}

// ensureStats computes and caches differs/added/removed in a single pass.
//
// Safe without locking: a DiffResult is owned by exactly one goroutine while
// being compared, and Bubble Tea runs Update and View sequentially, so there is
// never concurrent access to the same result.
func (r *DiffResult) ensureStats() {
	if r.statsReady {
		return
	}
	r.statsReady = true
	if r.LocalOnly || r.RemoteOnly || r.ContentDiff {
		r.differs = true
	}
	for _, l := range r.Lines {
		switch l.Kind {
		case LineAdded:
			r.added++
			r.differs = true
		case LineRemoved:
			r.removed++
			r.differs = true
		}
	}
}

// HasDiff returns true if the files differ in any way.
func (r *DiffResult) HasDiff() bool {
	r.ensureStats()
	return r.differs
}

// Counts returns the number of added and removed lines.
func (r *DiffResult) Counts() (added, removed int) {
	r.ensureStats()
	return r.added, r.removed
}

// Session pairs a browser selection entry with its loaded DiffResult.
type Session struct {
	LocalPath  string
	RemotePath string
	Result     *DiffResult
	Loaded     bool
	Err        error
}
