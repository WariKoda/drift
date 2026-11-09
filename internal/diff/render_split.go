package diff

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

func renderSplitRow(result *DiffResult, before, after, width, digits int, options RenderOptions) []string {
	if width <= 0 {
		return []string{""}
	}
	left := (width - 1) / 2
	lefts, leftBlank := renderSplitCell(result, before, left, digits, true, options)
	rights, rightBlank := renderSplitCell(result, after, width-left-1, digits, false, options)
	out := make([]string, max(len(lefts), len(rights)))
	for i := range out {
		l, r := leftBlank, rightBlank
		if i < len(lefts) {
			l = lefts[i]
		}
		if i < len(rights) {
			r = rights[i]
		}
		out[i] = l + styles.Sep.Render("│") + r
	}
	return out
}

// renderSplitCell returns the wrapped rows of one side and the blank row that
// continues it, in the line's own colours, when the other side is taller.
func renderSplitCell(result *DiffResult, index, width, digits int, before bool, options RenderOptions) ([]string, string) {
	if index < 0 || index >= len(result.Lines) {
		blank := strings.Repeat(" ", width)
		return []string{blank}, blank
	}
	line := result.Lines[index]
	number := line.RemoteNum
	if before != options.Flip {
		number = line.LocalNum
	}
	act := unifiedAction(line.Kind, options.Flip)
	if !options.Highlight {
		act = actEqual
	}
	style := styles.File
	marker := " "
	switch act {
	case actRemove:
		style = styles.DiffRemoved
		marker = "-"
	case actAdd:
		style = styles.DiffAdded
		marker = "+"
	}
	gutter := formatLineNum(number, digits) + " " + marker + " "
	contentWidth := max(0, width-digits-3)
	var syntax []SyntaxSpan
	if options.Syntax != nil {
		syntax = options.Syntax.Remote[index]
		if before != options.Flip {
			syntax = options.Syntax.Local[index]
		}
	}
	segments := renderSegments(line.Text, options.Spans[index], syntax, style, options, contentWidth)
	out := make([]string, len(segments))
	for i, content := range segments {
		content += strings.Repeat(" ", max(0, contentWidth-lipgloss.Width(content)))
		out[i] = pad(style.Render(gutter)+style.Render(content), width)
		gutter = strings.Repeat(" ", len(gutter))
	}
	return out, pad(style.Render(strings.Repeat(" ", max(0, width))), width)
}
