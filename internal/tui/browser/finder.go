package browser

import (
	"context"
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
	id      uint64
	query   string
	err     string

	rel     []string // file paths relative to WorkDir (display + match source)
	abs     []string // absolute paths, index-aligned with rel
	ignored []bool   // index-aligned classification for rendering
	hidden  int      // files omitted by visibility settings

	results []finderResult
	cursor  int
	offset  int
}

type finderResult struct {
	rel     string
	abs     string
	matched []int // rune indexes in rel that matched the query (for highlighting)
	ignored bool
}

// msgFinderIndex carries the result of the async project walk.
type msgFinderIndex struct {
	base    string
	id      uint64
	session *string
	rel     []string
	abs     []string
	ignored []bool
	hidden  int
	err     error
}

// buildFinderIndexCmd walks base and returns every file path (abs + relative).
func buildFinderIndexCmd(base string, classifier *fs.Classifier, showHidden, showIgnored bool, id uint64, session *string) tea.Cmd {
	return func() tea.Msg {
		var walked []string
		err := fs.WalkFiles(base, func(path string) error {
			walked = append(walked, path)
			return nil
		})
		if err != nil {
			return msgFinderIndex{base: base, id: id, session: session, err: err}
		}
		candidates := make([]fs.ClassifyCandidate, len(walked))
		for i, path := range walked {
			candidates[i] = fs.ClassifyCandidate{Path: path}
		}
		classes, err := classifier.ClassifyBatch(context.Background(), candidates)
		if err != nil {
			return msgFinderIndex{base: base, id: id, session: session, err: err}
		}
		var rel, abs []string
		var ignored []bool
		hidden := 0
		for i, path := range walked {
			class := classes[i]
			if class.HardExcluded {
				continue
			}
			if (!showHidden && class.Hidden) || (!showIgnored && class.Ignored) {
				hidden++
				continue
			}
			relative, relErr := filepath.Rel(base, path)
			if relErr != nil {
				relative = path
			}
			rel = append(rel, relative)
			abs = append(abs, path)
			ignored = append(ignored, class.Ignored)
		}
		return msgFinderIndex{base: base, id: id, session: session, rel: rel, abs: abs, ignored: ignored, hidden: hidden}
	}
}

func (f *finder) ignoredAt(index int) bool {
	return index >= 0 && index < len(f.ignored) && f.ignored[index]
}

// recompute rebuilds the results list from the current query.
func (f *finder) recompute() {
	f.results = f.results[:0]
	if f.query == "" {
		for i, r := range f.rel {
			f.results = append(f.results, finderResult{rel: r, abs: f.abs[i], ignored: f.ignoredAt(i)})
		}
	} else {
		for _, mt := range fuzzy.Find(f.query, f.rel) {
			f.results = append(f.results, finderResult{
				rel:     f.rel[mt.Index],
				abs:     f.abs[mt.Index],
				matched: mt.MatchedIndexes,
				ignored: f.ignoredAt(mt.Index),
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
// clampOffset keeps the result offset in range without dragging it back to the
// cursor. The wheel moves the viewport on its own, so the regular clamp — which
// exists to follow the cursor — must not run after it.
func (f *finder) clampOffset(vh int) {
	max := len(f.results) - vh
	if max < 0 {
		max = 0
	}
	if f.offset > max {
		f.offset = max
	}
	if f.offset < 0 {
		f.offset = 0
	}
}

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
