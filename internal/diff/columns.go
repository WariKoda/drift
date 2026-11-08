package diff

import (
	"context"

	"github.com/charmbracelet/x/ansi"
)

// MeasureColumns uses the renderer's exact tab and control-character projection.
func MeasureColumns(ctx context.Context, lines []DiffLine) (int, error) {
	width := 0
	for _, line := range lines {
		if err := ctx.Err(); err != nil {
			return 0, err
		}
		width = max(width, ansi.StringWidth(string(expandTabs([]rune(line.Text), 4))))
	}
	return width, nil
}
