package diff

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

func renderSplitRow(result *DiffResult, before, after, width, digits int, options RenderOptions) string {
	if width <= 0 {
		return ""
	}
	left := (width - 1) / 2
	return renderSplitCell(result, before, left, digits, true, options) + styles.Sep.Render("│") + renderSplitCell(result, after, width-left-1, digits, false, options)
}

func renderSplitCell(result *DiffResult, index, width, digits int, before bool, options RenderOptions) string {
	if index < 0 || index >= len(result.Lines) {
		return strings.Repeat(" ", width)
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
	content := renderText(line.Text, options.Spans[index], style, options, contentWidth)
	content += strings.Repeat(" ", max(0, contentWidth-lipgloss.Width(content)))
	return pad(style.Render(gutter)+style.Render(content), width)
}
