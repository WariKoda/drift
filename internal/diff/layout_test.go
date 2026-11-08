package diff

import (
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
)

func TestUnifiedContentFitsTerminalColumnsAndEscapesControls(t *testing.T) {
	for _, text := range []string{strings.Repeat("界", 80), strings.Repeat("👩‍💻é", 30), "\t\x1b]52;c;SGVsbG8=\a"} {
		result := &DiffResult{Lines: []DiffLine{{Text: text, Kind: LineAdded, RemoteNum: 1}}}
		for _, width := range []int{0, 1, 5, 10, 20, 40, 80} {
			rows := RenderUnifiedRows(result, Flatten(result.Lines, DefaultContext, nil, false), width, 0, 10, RenderOptions{Highlight: true})
			for _, row := range rows {
				if ansi.StringWidth(row) > width || strings.Contains(row, "\x1b]52;") {
					t.Fatalf("diff row escapes its %d columns: %q", width, row)
				}
			}
		}
	}
}
