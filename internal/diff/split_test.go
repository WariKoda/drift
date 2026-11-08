package diff

import (
	"reflect"
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
)

func TestSplitPairsUnequalBlocksWithoutChangingComparison(t *testing.T) {
	lines := lineDiff("same\nold one\nold two\nend\n", "same\nnew one\nend\n")
	original := append([]DiffLine(nil), lines...)
	result := &DiffResult{Lines: lines}
	for _, flip := range []bool{false, true} {
		rows := SplitDisplayRows(lines, Flatten(lines, DefaultContext, nil, flip), flip)
		pairs := 0
		for _, row := range rows {
			if row.Kind == DisplayPair {
				pairs++
				if row.SourceLine() < 0 {
					t.Fatal("pair has no source anchor")
				}
			}
		}
		if pairs != 2 {
			t.Fatalf("pairs=%d", pairs)
		}
		for _, width := range []int{0, 1, 10, 40, 101} {
			painted := RenderUnifiedRows(result, rows, width, 0, len(rows), RenderOptions{Split: true, Flip: flip, Highlight: true})
			for _, line := range painted {
				if ansi.StringWidth(line) != width {
					t.Fatalf("width=%d: %q", width, line)
				}
			}
		}
	}
	if !reflect.DeepEqual(lines, original) {
		t.Fatal("split modified comparison")
	}
}

func TestHorizontalRevealPreservesGutters(t *testing.T) {
	result := &DiffResult{Lines: []DiffLine{{Text: strings.Repeat("x", 100) + "REVEAL", Kind: LineAdded, RemoteNum: 10000}}}
	rows := Flatten(result.Lines, DefaultContext, nil, false)
	for _, split := range []bool{false, true} {
		planned := rows
		if split {
			planned = SplitDisplayRows(result.Lines, rows, false)
		}
		painted := RenderUnifiedRows(result, planned, 80, 0, len(planned), RenderOptions{Highlight: true, Offset: 100, Split: split})
		text := ansi.Strip(strings.Join(painted, "\n"))
		if !strings.Contains(text, "REVEAL") || !strings.Contains(text, "10000") {
			t.Fatalf("reveal lost text or gutter: %q", text)
		}
	}
}
