package diff

import (
	"slices"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

func TestWrapStartsBreakBetweenGraphemes(t *testing.T) {
	for _, tc := range []struct {
		text  string
		width int
		want  []int
	}{
		{strings.Repeat("a", 25), 10, []int{0, 10, 20}},
		{"日本語日本", 3, []int{0, 2, 4, 6, 8}},
		{"ab👩‍💻cd", 3, []int{0, 2, 5}},
		{"e\u0301e\u0301e\u0301", 2, []int{0, 2}},
		{"日本", 1, []int{0, 2}},
		{"", 5, []int{0}},
		{"abc", 0, []int{0}},
	} {
		if got := wrapStarts(projectText(tc.text), tc.width); !slices.Equal(got, tc.want) {
			t.Errorf("wrapStarts(%q, %d) = %v, want %v", tc.text, tc.width, got, tc.want)
		}
	}
	if got := wrapStarts(strings.Repeat("x", 1000), 1); len(got) != maxWrapRows {
		t.Fatalf("a 1000-cell line wrapped into %d rows, want the %d-row cap", len(got), maxWrapRows)
	}
}

func wrapResult(texts ...string) *DiffResult {
	lines := make([]DiffLine, len(texts))
	for i, text := range texts {
		lines[i] = DiffLine{Text: text, Kind: LineRemoved, LocalNum: i + 1}
		if i%2 == 1 {
			lines[i] = DiffLine{Text: text, Kind: LineAdded, RemoteNum: i}
		}
	}
	return &DiffResult{Lines: lines}
}

func TestWrappedUnifiedRowsKeepEveryCellOnce(t *testing.T) {
	text := "start\tfield 日本語 e\u0301 👩‍💻 \x1b[31m " + strings.Repeat("0123456789", 6) + " end"
	result := wrapResult(text)
	rows := []DisplayRow{{Kind: DisplayLine, LineIndex: 0}}
	options := RenderOptions{Highlight: true, Wrap: true}
	width := 30
	out := RenderRows(result, rows, width, 100, options)
	if len(out) != RowHeight(result, rows[0], width, options) || len(out) < 3 {
		t.Fatalf("rendered %d rows, RowHeight %d", len(out), RowHeight(result, rows[0], width, options))
	}
	var content strings.Builder
	for i, row := range out {
		if lipgloss.Width(row) != width || strings.Contains(row, "\x1b[31m ") {
			t.Fatalf("row %d is %d cells or unsafe: %q", i, lipgloss.Width(row), row)
		}
		plain := ansi.Strip(row)
		gutter := plain[:12]
		if i == 0 && !strings.Contains(gutter, "1") || i > 0 && strings.TrimSpace(gutter) != "" {
			t.Fatalf("row %d gutter %q", i, gutter)
		}
		content.WriteString(strings.TrimRight(plain[12:], " "))
	}
	want := strings.ReplaceAll(projectText(text), " ", "")
	if got := strings.ReplaceAll(content.String(), " ", ""); got != want {
		t.Fatalf("wrapped content %q, want %q", got, want)
	}
	if partial := RenderRows(result, rows, width, 2, options); len(partial) != 2 {
		t.Fatalf("budget of 2 rendered %d rows", len(partial))
	}
	unwrapped := RenderRows(result, rows, width, 100, RenderOptions{Highlight: true})
	if len(unwrapped) != 1 || RowHeight(result, rows[0], width, RenderOptions{}) != 1 {
		t.Fatal("without Wrap a line must stay one row")
	}
}

func TestWrappedSplitRowsFollowTheTallerSide(t *testing.T) {
	result := wrapResult("short", strings.Repeat("long ", 30))
	row := DisplayRow{Kind: DisplayPair, LineIndex: 0, PairIndex: 1}
	options := RenderOptions{Highlight: true, Split: true, Wrap: true}
	for _, width := range []int{40, 81, 160} {
		out := RenderRows(result, []DisplayRow{row}, width, 100, options)
		if len(out) != RowHeight(result, row, width, options) || len(out) < 2 {
			t.Fatalf("width %d: %d rows, RowHeight %d", width, len(out), RowHeight(result, row, width, options))
		}
		for _, line := range out {
			if lipgloss.Width(line) != width || !strings.Contains(line, "│") {
				t.Fatalf("width %d: misaligned row %q", width, ansi.Strip(line))
			}
		}
		if !strings.Contains(ansi.Strip(out[0]), "short") || strings.Contains(ansi.Strip(out[1]), "short") {
			t.Fatalf("width %d: short side repeated", width)
		}
	}
}

func TestWrapCapEndsWithEllipsis(t *testing.T) {
	result := wrapResult(strings.Repeat("x", 5000))
	rows := []DisplayRow{{Kind: DisplayLine, LineIndex: 0}}
	out := RenderRows(result, rows, 30, 1000, RenderOptions{Wrap: true})
	if len(out) != maxWrapRows || !strings.Contains(ansi.Strip(out[len(out)-1]), "…") {
		t.Fatalf("capped line rendered %d rows, last %q", len(out), ansi.Strip(out[len(out)-1]))
	}
}
