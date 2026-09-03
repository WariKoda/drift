package diff

import (
	"reflect"
	"testing"
)

func numberedEquals(n, localStart int) []DiffLine {
	out := make([]DiffLine, n)
	for i := range out {
		num := localStart + i
		out[i] = DiffLine{Text: "eq", Kind: LineEqual, LocalNum: num, RemoteNum: num}
	}
	return out
}

func TestFlattenIdenticalFileShowsAllLines(t *testing.T) {
	lines := numberedEquals(10, 1)
	rows := Flatten(lines, DefaultContext, nil)
	if len(rows) != 10 {
		t.Fatalf("got %d rows, want 10", len(rows))
	}
	for i, r := range rows {
		if r.Kind != DisplayLine || r.LineIndex != i {
			t.Fatalf("row %d = %+v, want DisplayLine %d", i, r, i)
		}
	}
}

func TestFlattenOnlyChangesHasHeader(t *testing.T) {
	lines := []DiffLine{
		{Text: "old", Kind: LineRemoved, LocalNum: 1},
		{Text: "new", Kind: LineAdded, RemoteNum: 1},
	}
	rows := Flatten(lines, DefaultContext, nil)
	if len(rows) != 3 {
		t.Fatalf("got %d rows, want header + 2 lines: %+v", len(rows), rows)
	}
	if rows[0].Kind != DisplayHunkHeader || rows[0].Header != "@@ -1,1 +1,1 @@" {
		t.Fatalf("header = %+v", rows[0])
	}
	if rows[1].LineIndex != 0 || rows[2].LineIndex != 1 {
		t.Fatalf("line indices = %d, %d", rows[1].LineIndex, rows[2].LineIndex)
	}
}

func TestFlattenShortGapStaysOpen(t *testing.T) {
	lines := numberedEquals(2, 1)
	lines = append(lines, DiffLine{Text: "old", Kind: LineRemoved, LocalNum: 3})
	lines = append(lines, numberedEquals(6, 4)...)
	lines = append(lines, DiffLine{Text: "gone", Kind: LineRemoved, LocalNum: 10})
	lines = append(lines, numberedEquals(2, 11)...)

	rows := Flatten(lines, DefaultContext, nil)
	for _, r := range rows {
		if r.Kind == DisplayFold {
			t.Fatalf("6-line gap should not fold: %+v", rows)
		}
	}
}

func TestFlattenSevenLineGapFoldsMiddle(t *testing.T) {
	lines := numberedEquals(2, 1)
	lines = append(lines, DiffLine{Text: "old", Kind: LineRemoved, LocalNum: 3})
	lines = append(lines, numberedEquals(7, 4)...)
	lines = append(lines, DiffLine{Text: "gone", Kind: LineRemoved, LocalNum: 11})
	lines = append(lines, numberedEquals(2, 12)...)

	rows := Flatten(lines, DefaultContext, nil)
	var fold DisplayRow
	found := false
	for _, r := range rows {
		if r.Kind == DisplayFold {
			fold = r
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected a fold, rows=%+v", rows)
	}
	// Change at index 2, then 7 equals starting at 3. GapID is 3.
	// prefix 3, suffix 3, hidden 1.
	if fold.GapID != 3 || fold.Hidden != 1 || fold.FoldStart != 6 || fold.FoldEnd != 7 {
		t.Fatalf("fold = %+v, want GapID=3 Hidden=1 [6,7)", fold)
	}
}

func TestFlattenLeadingAndTrailingFolds(t *testing.T) {
	lines := numberedEquals(7, 1)
	lines = append(lines, DiffLine{Text: "old", Kind: LineRemoved, LocalNum: 8})
	lines = append(lines, numberedEquals(7, 9)...)

	rows := Flatten(lines, DefaultContext, nil)
	kinds := make([]DisplayKind, len(rows))
	for i, r := range rows {
		kinds[i] = r.Kind
	}
	want := []DisplayKind{
		DisplayFold, DisplayLine, DisplayLine, DisplayLine,
		DisplayHunkHeader, DisplayLine,
		DisplayLine, DisplayLine, DisplayLine, DisplayFold,
	}
	if !reflect.DeepEqual(kinds, want) {
		t.Fatalf("kinds = %v, want %v", kinds, want)
	}
	if rows[0].Hidden != 4 || rows[0].FoldStart != 0 || rows[0].FoldEnd != 4 {
		t.Fatalf("leading fold = %+v, want hidden 4 covering [0,4)", rows[0])
	}
	if rows[len(rows)-1].Hidden != 4 {
		t.Fatalf("trailing fold hidden = %d, want 4", rows[len(rows)-1].Hidden)
	}
}

func TestFlattenExpandShowsFullGap(t *testing.T) {
	lines := numberedEquals(2, 1)
	lines = append(lines, DiffLine{Text: "old", Kind: LineRemoved, LocalNum: 3})
	lines = append(lines, numberedEquals(7, 4)...)
	lines = append(lines, DiffLine{Text: "gone", Kind: LineRemoved, LocalNum: 11})

	collapsed := Flatten(lines, DefaultContext, nil)
	var gapID int
	for _, r := range collapsed {
		if r.Kind == DisplayFold {
			gapID = r.GapID
			break
		}
	}
	expanded := Flatten(lines, DefaultContext, map[int]struct{}{gapID: {}})
	for _, r := range expanded {
		if r.Kind == DisplayFold && r.GapID == gapID {
			t.Fatalf("expanded gap still folded: %+v", r)
		}
	}
	equalCount := 0
	for _, r := range expanded {
		if r.Kind == DisplayLine && lines[r.LineIndex].Kind == LineEqual {
			equalCount++
		}
	}
	if equalCount != 9 {
		t.Fatalf("equal lines shown = %d, want 9", equalCount)
	}
}

func TestFlattenHunkHeaderIncludesContext(t *testing.T) {
	lines := []DiffLine{
		{Text: "a", Kind: LineEqual, LocalNum: 1, RemoteNum: 1},
		{Text: "old", Kind: LineRemoved, LocalNum: 2},
		{Text: "new", Kind: LineAdded, RemoteNum: 2},
		{Text: "c", Kind: LineEqual, LocalNum: 3, RemoteNum: 3},
	}
	rows := Flatten(lines, DefaultContext, nil)
	if rows[1].Kind != DisplayHunkHeader || rows[1].Header != "@@ -1,3 +1,3 @@" {
		t.Fatalf("header = %+v, want @@ -1,3 +1,3 @@ after first context line", rows[1])
	}
}

func TestIndexOfSourceLineFindsFoldAndLine(t *testing.T) {
	lines := numberedEquals(7, 1)
	lines = append(lines, DiffLine{Text: "old", Kind: LineRemoved, LocalNum: 8})
	rows := Flatten(lines, DefaultContext, nil)
	if got := IndexOfSourceLine(rows, 0); got != 0 {
		t.Fatalf("hidden line 0 -> %d, want fold at 0", got)
	}
	if got := IndexOfSourceLine(rows, 6); rows[got].Kind != DisplayLine || rows[got].LineIndex != 6 {
		t.Fatalf("visible line 6 -> row %+v", rows[got])
	}
	if got := IndexOfSourceLine(rows, 7); rows[got].Kind != DisplayLine || rows[got].LineIndex != 7 {
		t.Fatalf("change line 7 -> row %+v", rows[got])
	}
}

func TestFoldableGapIDs(t *testing.T) {
	lines := numberedEquals(7, 1)
	lines = append(lines, DiffLine{Text: "old", Kind: LineRemoved, LocalNum: 8})
	lines = append(lines, numberedEquals(7, 9)...)
	got := FoldableGapIDs(lines, DefaultContext)
	if !reflect.DeepEqual(got, []int{0, 8}) {
		t.Fatalf("gap IDs = %v, want [0 8]", got)
	}
}
