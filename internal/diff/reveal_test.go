package diff

import "testing"

// revealFixture: 20 equal, change, 30 equal, change, 20 equal.
func revealFixture() []DiffLine {
	lines := numberedEquals(20, 1)
	lines = append(lines, DiffLine{Text: "a", Kind: LineRemoved, LocalNum: 21})
	lines = append(lines, numberedEquals(30, 22)...)
	lines = append(lines, DiffLine{Text: "b", Kind: LineRemoved, LocalNum: 52})
	return append(lines, numberedEquals(20, 53)...)
}

func shownLines(rows []DisplayRow) map[int]bool {
	shown := map[int]bool{}
	for _, row := range rows {
		if row.Kind == DisplayLine {
			shown[row.LineIndex] = true
		}
	}
	return shown
}

func folds(rows []DisplayRow) map[int]int {
	hidden := map[int]int{}
	for _, row := range rows {
		if row.Kind == DisplayFold {
			hidden[row.GapID] = row.Hidden
		}
	}
	return hidden
}

func TestFlattenWithRevealGrowsOnlySidesThatBorderChanges(t *testing.T) {
	lines := revealFixture()
	base := folds(FlattenWith(lines, 3, nil, nil, false))
	rows := FlattenWith(lines, 3, nil, map[int]int{0: 5, 21: 5, 52: 5}, false)
	got := folds(rows)
	for gap, want := range map[int]int{0: base[0] - 5, 21: base[21] - 10, 52: base[52] - 5} {
		if got[gap] != want {
			t.Fatalf("gap %d hides %d lines, want %d (base %v, got %v)", gap, got[gap], want, base, got)
		}
	}
	shown := shownLines(rows)
	for _, index := range []int{12, 19, 21, 28, 43, 50, 52, 59} {
		if !shown[index] {
			t.Fatalf("line %d should be revealed", index)
		}
	}
	for _, index := range []int{11, 29, 42, 60} {
		if shown[index] {
			t.Fatalf("line %d should stay folded", index)
		}
	}
}

func TestFlattenWithRevealCoveringARunRemovesItsFold(t *testing.T) {
	lines := revealFixture()
	rows := FlattenWith(lines, 3, nil, map[int]int{21: 12, -1: 99, 20: 99}, false)
	if _, ok := folds(rows)[21]; ok {
		t.Fatal("30-line run with 15 lines of context per side kept a fold")
	}
	if len(folds(rows)) != 2 {
		t.Fatalf("reveal for a non-gap or unknown id changed other folds: %v", folds(rows))
	}
	// Each change run keeps its header; an unfolded run belongs to both ranges.
	headers := headerIndexes(rows)
	if len(headers) != 2 || rows[headers[0]].Header != "@@ -18,34 +18,33 @@" || rows[headers[1]].Header != "@@ -22,34 +22,33 @@" {
		t.Fatalf("hunk headers = %+v", rows)
	}
	if negative := FlattenWith(lines, 3, nil, map[int]int{21: -10}, false); len(negative) != len(Flatten(lines, 3, nil, false)) {
		t.Fatal("negative reveal shrank the context")
	}
}
