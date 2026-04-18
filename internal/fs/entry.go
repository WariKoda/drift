// Package fs defines the shared FileEntry type used for both local and remote files.
package fs

import (
	"io/fs"
	"time"
)

// EntryKind classifies a filesystem entry.
type EntryKind int

const (
	EntryFile EntryKind = iota
	EntryDir
	EntrySymlink
)

// FileEntry represents a single file or directory, local or remote.
type FileEntry struct {
	Name    string
	Path    string // absolute path (local or remote)
	Kind    EntryKind
	Size    int64
	ModTime time.Time
	Mode    fs.FileMode

	// Tree metadata (used by the browser TUI)
	Depth    int
	Expanded bool // directories only
	Children []*FileEntry
	Parent   *FileEntry
}

// SelectionState tracks which entries are marked by the user.
type SelectionState struct {
	Marked map[string]struct{} // keyed by FileEntry.Path
}

// NewSelectionState returns an empty SelectionState.
func NewSelectionState() *SelectionState {
	return &SelectionState{Marked: make(map[string]struct{})}
}

// Toggle marks an entry if unmarked, or unmarks it if already marked.
func (s *SelectionState) Toggle(path string) {
	if _, ok := s.Marked[path]; ok {
		delete(s.Marked, path)
	} else {
		s.Marked[path] = struct{}{}
	}
}

// IsMarked returns true if the path is currently marked.
func (s *SelectionState) IsMarked(path string) bool {
	_, ok := s.Marked[path]
	return ok
}

// Clear removes all marks.
func (s *SelectionState) Clear() {
	s.Marked = make(map[string]struct{})
}

// Count returns the number of marked entries.
func (s *SelectionState) Count() int {
	return len(s.Marked)
}
