package browser

import (
	"path/filepath"

	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/sahilm/fuzzy"
)

// finder is the project-wide fuzzy file finder overlay (opened with [f]).
//
// It walks the whole project once (respecting the same skip rules as the tree)
// and lets the user fuzzy-search and multi-select files to mark for sync. Marks
// are written straight to the shared Selection, so a file can be marked even if
// its directory is never expanded in the tree.
type finder struct {
	active  bool
	loading bool
	query   string

	rel []string // file paths relative to WorkDir (display + match source)
	abs []string // absolute paths, index-aligned with rel

	results []finderResult
	cursor  int
	offset  int
}

type finderResult struct {
	rel     string
	abs     string
	matched []int // rune indexes in rel that matched the query (for highlighting)
}

// msgFinderIndex carries the result of the async project walk.
type msgFinderIndex struct {
	base string
	rel  []string
	abs  []string
}

// buildFinderIndexCmd walks base and returns every file path (abs + relative).
func buildFinderIndexCmd(base string) tea.Cmd {
	return func() tea.Msg {
		var rel, abs []string
		_ = fs.WalkFiles(base, func(p string) error {
			r, err := filepath.Rel(base, p)
			if err != nil {
				r = p
			}
			rel = append(rel, r)
			abs = append(abs, p)
			return nil
		})
		return msgFinderIndex{base: base, rel: rel, abs: abs}
	}
}

// recompute rebuilds the results list from the current query.
func (f *finder) recompute() {
	f.results = f.results[:0]
	if f.query == "" {
		for i, r := range f.rel {
			f.results = append(f.results, finderResult{rel: r, abs: f.abs[i]})
		}
	} else {
		for _, mt := range fuzzy.Find(f.query, f.rel) {
			f.results = append(f.results, finderResult{
				rel:     f.rel[mt.Index],
				abs:     f.abs[mt.Index],
				matched: mt.MatchedIndexes,
			})
		}
	}
	f.offset = 0
	if f.cursor >= len(f.results) {
		f.cursor = len(f.results) - 1
	}
	if f.cursor < 0 {
		f.cursor = 0
	}
}

// current returns the result under the cursor, or nil.
func (f *finder) current() *finderResult {
	if f.cursor < 0 || f.cursor >= len(f.results) {
		return nil
	}
	return &f.results[f.cursor]
}

// clamp keeps the cursor in bounds and scrolled into the vh-row window.
func (f *finder) clamp(vh int) {
	if len(f.results) == 0 {
		f.cursor, f.offset = 0, 0
		return
	}
	if f.cursor < 0 {
		f.cursor = 0
	}
	if f.cursor >= len(f.results) {
		f.cursor = len(f.results) - 1
	}
	if f.cursor < f.offset {
		f.offset = f.cursor
	}
	if vh > 0 && f.cursor >= f.offset+vh {
		f.offset = f.cursor - vh + 1
	}
	if f.offset < 0 {
		f.offset = 0
	}
}
