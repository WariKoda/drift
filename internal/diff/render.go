package diff

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// RenderOptions contains only presentation choices for an immutable comparison.
type RenderOptions struct {
	Flip, Highlight bool
	Spans           map[int][]TextSpan
	Offset          int // ignored when Wrap is set
	Split           bool
	Syntax          *SyntaxHighlights
	Wrap            bool // continue long lines on further rows instead of clipping
}

// RenderUnifiedRows renders flattened display rows (hunk headers, folds, lines).
// scrollOffset is the first DisplayRow index; count is the number of viewport rows.
// Use the same flip value as Flatten so headers, row order and gutters agree.
// highlight enables addition/removal colours and markers for planned actions.
func RenderUnifiedRows(result *DiffResult, rows []DisplayRow, width, scrollOffset, count int, options RenderOptions) []string {
	width, count = max(0, width), max(0, count)
	scrollOffset = max(0, scrollOffset)
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

	out = append(out, RenderRows(result, rows[min(scrollOffset, len(rows)):], width, count, options)...)
	for len(out) < count {
		out = append(out, strings.Repeat(" ", width))
	}
	return out
}

// RenderRows renders rows from the first one until budget terminal rows are
// filled or the rows run out, without padding. A wrapped line may end in the
// middle when the budget runs out.
func RenderRows(result *DiffResult, rows []DisplayRow, width, budget int, options RenderOptions) []string {
	width, budget = max(0, width), max(0, budget)
	out := make([]string, 0, min(budget, len(rows)))
	numWidth := result.LineNumberWidth()
	contentWidth := max(0, width-2*numWidth-4)
	for idx := 0; idx < len(rows) && len(out) < budget; idx++ {
		var rendered []string
		switch rows[idx].Kind {
		case DisplayHunkHeader:
			rendered = []string{renderHunkHeader(rows[idx].Header, rows[idx].Label, width)}
		case DisplayFold:
			rendered = []string{renderFold(rows[idx].Hidden, width)}
		case DisplayPair:
			rendered = renderSplitRow(result, rows[idx].LineIndex, rows[idx].PairIndex, width, numWidth, options)
		default:
			li := rows[idx].LineIndex
			switch {
			case li < 0 || li >= len(result.Lines):
				rendered = []string{strings.Repeat(" ", width)}
			case options.Split:
				rendered = renderSplitRow(result, li, li, width, numWidth, options)
			default:
				rendered = renderUnifiedLine(result.Lines[li], options, width, numWidth, contentWidth, options.Spans[li], unifiedSyntax(result.Lines[li], li, options))
			}
		}
		out = append(out, rendered[:min(len(rendered), budget-len(out))]...)
	}
	return out
}

func unifiedSyntax(line DiffLine, index int, options RenderOptions) []SyntaxSpan {
	if options.Syntax == nil {
		return nil
	}
	if line.Kind == LineRemoved || line.Kind == LineEqual && options.Flip {
		return options.Syntax.Local[index]
	}
	return options.Syntax.Remote[index]
}

// RowHeight is the number of terminal rows RenderRows uses for row.
func RowHeight(result *DiffResult, row DisplayRow, width int, options RenderOptions) int {
	if !options.Wrap || result == nil {
		return 1
	}
	numWidth := result.LineNumberWidth()
	height := func(index, contentWidth int) int {
		if index < 0 || index >= len(result.Lines) {
			return 1
		}
		return len(wrapStarts(projectText(result.Lines[index].Text), contentWidth))
	}
	switch {
	case row.Kind == DisplayPair || row.Kind == DisplayLine && options.Split:
		left := (width - 1) / 2
		after := row.PairIndex
		if row.Kind == DisplayLine {
			after = row.LineIndex
		}
		return max(height(row.LineIndex, max(0, left-numWidth-3)), height(after, max(0, width-left-1-numWidth-3)))
	case row.Kind == DisplayLine:
		return height(row.LineIndex, max(0, width-2*numWidth-4))
	default:
		return 1
	}
}

// renderHunkHeader shortens the label first; the line ranges stay complete
// whenever they fit.
func renderHunkHeader(header, label string, width int) string {
	inner := " " + header + " "
	if room := width - lipgloss.Width(inner) - 5; label != "" && room >= 4 {
		inner += ansi.Truncate(string(expandTabs([]rune(label), 4)), room, "…") + " "
	}
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

func renderUnifiedLine(dl DiffLine, options RenderOptions, width, numWidth, contentWidth int, spans []TextSpan, syntax []SyntaxSpan) []string {
	act := unifiedAction(dl.Kind, options.Flip)
	if !options.Highlight {
		act = actEqual
	}

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

	before, after := dl.LocalNum, dl.RemoteNum
	if options.Flip {
		before, after = after, before
	}
	gutter := formatLineNum(before, numWidth) + " " + formatLineNum(after, numWidth) + " "
	segments := renderSegments(dl.Text, spans, syntax, textStyle, options, contentWidth)
	out := make([]string, len(segments))
	for i, content := range segments {
		content += strings.Repeat(" ", max(0, contentWidth-lipgloss.Width(content)))
		line := numStyle.Render(gutter) + textStyle.Bold(true).Render(marker) + textStyle.Render(" "+content)
		out[i] = pad(line, width) + textStyle.Render("")
		gutter, marker = strings.Repeat(" ", len(gutter)), " "
	}
	return out
}

func pad(s string, width int) string {
	width = max(0, width)
	w := lipgloss.Width(s)
	if w > width {
		return ansi.Truncate(s, width, "")
	}
	if w == width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

func truncateRunes(s string, n int) string {
	return ansi.Truncate(styles.SafeText(string(expandTabs([]rune(s), 4))), max(0, n), "")
}

func expandTabs(r []rune, tabWidth int) []rune {
	var out strings.Builder
	col := 0
	segments := strings.Split(strings.ReplaceAll(string(r), "\r", ""), "\t")
	for i, segment := range segments {
		if i > 0 {
			spaces := max(1, tabWidth) - col%max(1, tabWidth)
			out.WriteString(strings.Repeat(" ", spaces))
			col += spaces
		}
		segment = styles.SafeText(segment)
		out.WriteString(segment)
		col += ansi.StringWidth(segment)
	}
	return []rune(out.String())
}
