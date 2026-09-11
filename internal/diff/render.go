package diff

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// RenderUnifiedRows renders flattened display rows (hunk headers, folds, lines).
// scrollOffset is the first DisplayRow index; count is the number of viewport rows.
// Use the same flip value as Flatten so headers, row order and gutters agree.
func RenderUnifiedRows(result *DiffResult, rows []DisplayRow, width, scrollOffset, count int, flip bool) []string {
	out := make([]string, 0, count)
	if result == nil {
		for i := 0; i < count; i++ {
			out = append(out, strings.Repeat(" ", width))
		}
		return out
	}

	if result.Binary {
		meta := fmt.Sprintf("  binary file  local %s %d bytes  remote %s %d bytes",
			result.ModLocal.Format("2006-01-02 15:04"), result.SizeLocal,
			result.ModRemote.Format("2006-01-02 15:04"), result.SizeRemote)
		if count > 0 {
			out = append(out, pad(styles.Muted.Render(meta), width))
		}
		for i := 1; i < count; i++ {
			out = append(out, strings.Repeat(" ", width))
		}
		return out
	}

	numWidth := 4
	contentWidth := width - 2*numWidth - 4
	if contentWidth < 1 {
		contentWidth = 1
	}

	for i := 0; i < count; i++ {
		idx := scrollOffset + i
		if idx >= len(rows) {
			out = append(out, strings.Repeat(" ", width))
			continue
		}
		switch rows[idx].Kind {
		case DisplayHunkHeader:
			out = append(out, renderHunkHeader(rows[idx].Header, width))
		case DisplayFold:
			out = append(out, renderFold(rows[idx].Hidden, width))
		default:
			li := rows[idx].LineIndex
			if li < 0 || li >= len(result.Lines) {
				out = append(out, strings.Repeat(" ", width))
				continue
			}
			out = append(out, renderUnifiedLine(result.Lines[li], flip, width, numWidth, contentWidth))
		}
	}
	return out
}

func renderHunkHeader(header string, width int) string {
	inner := " " + header + " "
	innerW := lipgloss.Width(inner)
	if innerW >= width {
		return pad(styles.DiffHunkHeader.Render(truncateRunes(inner, width)), width)
	}
	remain := width - innerW
	left := remain / 2
	right := remain - left
	line := styles.Sep.Render(strings.Repeat("┄", left)) +
		styles.Muted.Render(inner) +
		styles.Sep.Render(strings.Repeat("┄", right))
	return pad(line, width)
}

func renderFold(hidden, width int) string {
	label := fmt.Sprintf("▸  %d unchanged lines", hidden)
	return pad(styles.DiffFold.Render(label), width)
}

// lineAct is how a unified diff line should be styled after applying sync direction.
type lineAct int

const (
	actEqual lineAct = iota
	actAdd
	actRemove
)

// unifiedAction resolves marker/colour for one DiffLine given sync direction.
// flip=false treats remote as the new state; flip=true treats local as new.
func unifiedAction(kind LineKind, flip bool) lineAct {
	switch kind {
	case LineRemoved:
		if flip {
			return actAdd
		}
		return actRemove
	case LineAdded:
		if flip {
			return actRemove
		}
		return actAdd
	default:
		return actEqual
	}
}

func formatLineNum(n, width int) string {
	if n <= 0 {
		return strings.Repeat(" ", width)
	}
	return fmt.Sprintf("%*d", width, n)
}

func renderUnifiedLine(dl DiffLine, flip bool, width, numWidth, contentWidth int) string {
	act := unifiedAction(dl.Kind, flip)

	textStyle := styles.File
	numStyle := styles.Muted
	marker := " "

	switch act {
	case actAdd:
		textStyle = styles.DiffAdded
		numStyle = styles.DiffAdded
		marker = "+"
	case actRemove:
		textStyle = styles.DiffRemoved
		numStyle = styles.DiffRemoved
		marker = "-"
	}

	content := truncateRunes(dl.Text, contentWidth)
	content = content + strings.Repeat(" ", contentWidth-lipgloss.Width(content))

	before, after := dl.LocalNum, dl.RemoteNum
	if flip {
		before, after = after, before
	}
	gutter := formatLineNum(before, numWidth) + " " + formatLineNum(after, numWidth) + " "
	line := numStyle.Render(gutter) + textStyle.Bold(true).Render(marker) + textStyle.Render(" "+content)
	return line + textStyle.Render(strings.Repeat(" ", max(0, width-lipgloss.Width(line))))
}

func pad(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

func truncateRunes(s string, n int) string {
	r := []rune(s)
	expanded := expandTabs(r, 4)
	if len(expanded) <= n {
		return string(expanded)
	}
	return string(expanded[:n])
}

func expandTabs(r []rune, tabWidth int) []rune {
	var out []rune
	col := 0
	for _, c := range r {
		if c == '\r' {
			continue
		}
		if c == '\t' {
			spaces := tabWidth - (col % tabWidth)
			for i := 0; i < spaces; i++ {
				out = append(out, ' ')
				col++
			}
		} else {
			out = append(out, c)
			col++
		}
	}
	return out
}
