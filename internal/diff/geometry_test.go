package diff

import (
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
)

func TestLineNumbersFitAndRemainStable(t *testing.T) {
	for _, number := range []int{9999, 10000, 100000} {
		result := &DiffResult{Lines: []DiffLine{
			{Kind: LineEqual, Text: strings.Repeat("x", 100), LocalNum: 1, RemoteNum: 1},
			{Kind: LineAdded, Text: "end", RemoteNum: number},
		}}
		for _, flip := range []bool{false, true} {
			for _, highlight := range []bool{false, true} {
				for _, width := range []int{0, 1, 10, 80} {
					rows := RenderUnifiedRows(result, Flatten(result.Lines, DefaultContext, nil, flip), width, 0, 6, RenderOptions{Flip: flip, Highlight: highlight})
					for _, row := range rows {
						if ansi.StringWidth(row) > width {
							t.Fatalf("number=%d width=%d: %q", number, width, row)
						}
					}
				}
			}
		}
		if result.LineNumberWidth() < len(strings.TrimSpace(formatLineNum(number, 1))) {
			t.Fatal("gutter ignores offscreen numbers")
		}
	}
}

func TestTabsFollowTerminalCells(t *testing.T) {
	for _, test := range []struct{ input, want string }{
		{"a\tb", "a   b"},
		{"日本\tx", "日本    x"},
		{"e\u0301\tx", "e\u0301   x"},
		{"👩‍💻\tx", "👩‍💻  x"},
		{"\x1b\tx", "\\x1b    x"},
	} {
		if got := string(expandTabs([]rune(test.input), 4)); got != test.want {
			t.Errorf("%q: got %q, want %q", test.input, got, test.want)
		}
	}
}
