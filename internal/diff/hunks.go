package diff

import "fmt"

// DefaultContext is the number of unchanged lines kept beside each change hunk.
const DefaultContext = 3

// DisplayKind classifies a row in the folded unified view.
type DisplayKind int

const (
	DisplayLine DisplayKind = iota
	DisplayHunkHeader
	DisplayFold
)

// DisplayRow is one viewport row after hunk headers and context folding.
type DisplayRow struct {
	Kind      DisplayKind
	LineIndex int    // Lines index for DisplayLine; change-run start for DisplayHunkHeader
	Header    string // @@ -l,s +r,s @@ for DisplayHunkHeader
	GapID     int    // start index of the equal run (stable across expand)
	Hidden    int    // folded equal-line count
	FoldStart int    // first hidden Lines index (inclusive)
	FoldEnd   int    // first Lines index after the fold (exclusive)
}

// SourceLine is the DiffLine index this row represents for scroll anchoring.
func (r DisplayRow) SourceLine() int {
	switch r.Kind {
	case DisplayFold:
		return r.FoldStart
	default:
		return r.LineIndex
	}
}

// Flatten builds display rows: hunk headers, limited context, and fold markers.
// expanded holds GapIDs whose equal runs should be shown in full.
func Flatten(lines []DiffLine, context int, expanded map[int]struct{}) []DisplayRow {
	if context < 0 {
		context = 0
	}
	if len(lines) == 0 {
		return nil
	}

	runs := splitRuns(lines)
	if !hasChangeRun(runs) {
		out := make([]DisplayRow, len(lines))
		for i := range lines {
			out[i] = DisplayRow{Kind: DisplayLine, LineIndex: i}
		}
		return out
	}

	foldBudget := 2 * context
	out := make([]DisplayRow, 0, len(lines)+len(runs))
	for i, r := range runs {
		if r.equal {
			out = append(out, flattenEqualRun(runs, i, context, foldBudget, expanded)...)
			continue
		}
		hStart, hEnd := hunkRange(runs, i, context, foldBudget, expanded)
		out = append(out, DisplayRow{
			Kind:      DisplayHunkHeader,
			LineIndex: r.start,
			Header:    formatHunkHeader(lines, hStart, hEnd),
		})
		for idx := r.start; idx < r.end; idx++ {
			out = append(out, DisplayRow{Kind: DisplayLine, LineIndex: idx})
		}
	}
	return out
}

// IndexOfSourceLine returns the display index covering lineIdx, or 0 if none.
func IndexOfSourceLine(rows []DisplayRow, lineIdx int) int {
	best := 0
	for i, r := range rows {
		switch r.Kind {
		case DisplayLine:
			if r.LineIndex == lineIdx {
				return i
			}
			if r.LineIndex < lineIdx {
				best = i
			}
		case DisplayFold:
			if lineIdx >= r.FoldStart && lineIdx < r.FoldEnd {
				return i
			}
		}
	}
	return best
}

// FoldableGapIDs lists equal-run starts that collapse under the default view.
func FoldableGapIDs(lines []DiffLine, context int) []int {
	var ids []int
	for _, row := range Flatten(lines, context, nil) {
		if row.Kind == DisplayFold {
			ids = append(ids, row.GapID)
		}
	}
	return ids
}

type run struct {
	equal      bool
	start, end int
}

func splitRuns(lines []DiffLine) []run {
	if len(lines) == 0 {
		return nil
	}
	var runs []run
	start := 0
	equal := lines[0].Kind == LineEqual
	for i := 1; i < len(lines); i++ {
		isEqual := lines[i].Kind == LineEqual
		if isEqual == equal {
			continue
		}
		runs = append(runs, run{equal: equal, start: start, end: i})
		start = i
		equal = isEqual
	}
	return append(runs, run{equal: equal, start: start, end: len(lines)})
}

func hasChangeRun(runs []run) bool {
	for _, r := range runs {
		if !r.equal {
			return true
		}
	}
	return false
}

func flattenEqualRun(runs []run, i, context, foldBudget int, expanded map[int]struct{}) []DisplayRow {
	r := runs[i]
	length := r.end - r.start
	_, shown := expanded[r.start]
	if length <= foldBudget || shown {
		out := make([]DisplayRow, 0, length)
		for idx := r.start; idx < r.end; idx++ {
			out = append(out, DisplayRow{Kind: DisplayLine, LineIndex: idx})
		}
		return out
	}

	prefix, suffix := equalContext(runs, i, context)
	out := make([]DisplayRow, 0, prefix+suffix+1)
	for idx := r.start; idx < r.start+prefix; idx++ {
		out = append(out, DisplayRow{Kind: DisplayLine, LineIndex: idx})
	}
	hiddenStart := r.start + prefix
	hiddenEnd := r.end - suffix
	out = append(out, DisplayRow{
		Kind:      DisplayFold,
		GapID:     r.start,
		Hidden:    hiddenEnd - hiddenStart,
		FoldStart: hiddenStart,
		FoldEnd:   hiddenEnd,
	})
	for idx := hiddenEnd; idx < r.end; idx++ {
		out = append(out, DisplayRow{Kind: DisplayLine, LineIndex: idx})
	}
	return out
}

func equalContext(runs []run, i, context int) (prefix, suffix int) {
	hasPrev := i > 0 && !runs[i-1].equal
	hasNext := i+1 < len(runs) && !runs[i+1].equal
	switch {
	case hasPrev && hasNext:
		return context, context
	case hasNext:
		return 0, context
	case hasPrev:
		return context, 0
	default:
		return 0, 0
	}
}

func hunkRange(runs []run, i, context, foldBudget int, expanded map[int]struct{}) (start, end int) {
	r := runs[i]
	start, end = r.start, r.end
	if i > 0 && runs[i-1].equal {
		prev := runs[i-1]
		length := prev.end - prev.start
		_, shown := expanded[prev.start]
		if length <= foldBudget || shown {
			start = prev.start
		} else {
			start = prev.end - context
		}
	}
	if i+1 < len(runs) && runs[i+1].equal {
		next := runs[i+1]
		length := next.end - next.start
		_, shown := expanded[next.start]
		if length <= foldBudget || shown {
			end = next.end
		} else {
			end = next.start + context
		}
	}
	return start, end
}

func formatHunkHeader(lines []DiffLine, start, end int) string {
	oldStart, newStart := 0, 0
	oldCount, newCount := 0, 0
	for i := start; i < end && i < len(lines); i++ {
		l := lines[i]
		switch l.Kind {
		case LineEqual:
			oldCount++
			newCount++
			if oldStart == 0 && l.LocalNum > 0 {
				oldStart = l.LocalNum
			}
			if newStart == 0 && l.RemoteNum > 0 {
				newStart = l.RemoteNum
			}
		case LineRemoved:
			oldCount++
			if oldStart == 0 && l.LocalNum > 0 {
				oldStart = l.LocalNum
			}
		case LineAdded:
			newCount++
			if newStart == 0 && l.RemoteNum > 0 {
				newStart = l.RemoteNum
			}
		}
	}
	return fmt.Sprintf("@@ -%d,%d +%d,%d @@", oldStart, oldCount, newStart, newCount)
}
