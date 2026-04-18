// Package diff compares local and remote files and produces structured diff results.
package diff

import "time"

// LineKind classifies a line in a side-by-side diff.
type LineKind int

const (
	LineEqual LineKind = iota
	LineAdded          // present on one side, absent on the other
	LineRemoved
	LineModified
)

// DiffLine holds one line of a side-by-side diff.
type DiffLine struct {
	LocalLine  string
	RemoteLine string
	Kind       LineKind
	LocalNum   int // 0 = line not present on this side
	RemoteNum  int
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
	Binary     bool
	SizeLocal  int64
	SizeRemote int64
	ModLocal   time.Time
	ModRemote  time.Time
}

// HasDiff returns true if the files differ in any way.
func (r *DiffResult) HasDiff() bool {
	if r.LocalOnly || r.RemoteOnly {
		return true
	}
	for _, l := range r.Lines {
		if l.Kind != LineEqual {
			return true
		}
	}
	return false
}

// Session pairs a browser selection entry with its loaded DiffResult.
type Session struct {
	LocalPath  string
	RemotePath string
	Result     *DiffResult
	Loaded     bool
	Err        error
}
