package diff

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/rivo/uniseg"
)

// maxWrapRows bounds one wrapped line; the last row ends in an ellipsis.
const maxWrapRows = 100

// projectText is the cell projection shared by drawing, wrapping and measuring.
func projectText(text string) string { return string(expandTabs([]rune(text), 4)) }

// wrapStarts returns the first cell of each wrapped row. Rows break between
// grapheme clusters, so a wide character never straddles two rows.
func wrapStarts(projected string, width int) []int {
	starts := []int{0}
	if width <= 0 {
		return starts
	}
	column, row := 0, 0
	g := uniseg.NewGraphemes(projected)
	for g.Next() {
		cells := g.Width()
		if column > row && column-row+cells > width {
			if len(starts) == maxWrapRows {
				break
			}
			row = column
			starts = append(starts, row)
		}
		column += cells
	}
	return starts
}

func renderText(text string, spans []TextSpan, syntax []SyntaxSpan, base lipgloss.Style, options RenderOptions, width int) string {
	return ansi.Cut(styledText(text, spans, syntax, base, options), max(0, options.Offset), max(0, options.Offset)+width)
}

// renderSegments returns the clipped line, or with options.Wrap every row of
// the wrapped line.
func renderSegments(text string, spans []TextSpan, syntax []SyntaxSpan, base lipgloss.Style, options RenderOptions, width int) []string {
	if !options.Wrap {
		return []string{renderText(text, spans, syntax, base, options, width)}
	}
	styled := styledText(text, spans, syntax, base, options)
	projected := projectText(text)
	starts := wrapStarts(projected, width)
	total := ansi.StringWidth(projected)
	out := make([]string, len(starts))
	for i, start := range starts {
		end := total
		if i+1 < len(starts) {
			end = starts[i+1]
		}
		out[i] = ansi.Cut(styled, start, end)
		if end-start > width {
			out[i] = ansi.Truncate(out[i], width, "…")
		}
	}
	return out
}

// styledText projects the whole line with word and syntax styles applied.
func styledText(text string, spans []TextSpan, syntax []SyntaxSpan, base lipgloss.Style, options RenderOptions) string {
	if !options.Highlight {
		spans = nil
	}
	if len(spans) == 0 && len(syntax) == 0 {
		return projectText(text)
	}
	var out, run strings.Builder
	column := 0
	marked := false
	class := SyntaxPlain
	spanIndex, syntaxIndex := 0, 0
	flush := func() {
		if run.Len() == 0 {
			return
		}
		style := base
		switch class {
		case SyntaxKeyword:
			style = styles.SyntaxKeyword.Inherit(base)
		case SyntaxString:
			style = styles.SyntaxString.Inherit(base)
		case SyntaxNumber:
			style = styles.SyntaxNumber.Inherit(base)
		case SyntaxComment:
			style = styles.SyntaxComment.Inherit(base)
		case SyntaxName:
			style = styles.SyntaxName.Inherit(base)
		}
		if marked {
			style = style.Inherit(styles.DiffEmphasis)
		}
		out.WriteString(style.Render(run.String()))
		run.Reset()
	}
	g := uniseg.NewGraphemes(text)
	for g.Next() {
		start, end := g.Positions()
		emphasis := false
		for spanIndex < len(spans) && spans[spanIndex].End <= start {
			spanIndex++
		}
		if spanIndex < len(spans) {
			span := spans[spanIndex]
			emphasis = span.Emphasis && start < span.End && end > span.Start
		}
		for syntaxIndex < len(syntax) && syntax[syntaxIndex].End <= start {
			syntaxIndex++
		}
		nextClass := SyntaxPlain
		if syntaxIndex < len(syntax) && syntax[syntaxIndex].Start < end {
			nextClass = syntax[syntaxIndex].Class
		}
		if emphasis != marked || nextClass != class {
			flush()
			marked, class = emphasis, nextClass
		}
		part := strings.ReplaceAll(g.Str(), "\r", "")
		if part == "" {
			continue
		}
		if part == "\t" {
			part = strings.Repeat(" ", 4-column%4)
		} else {
			part = styles.SafeText(part)
		}
		run.WriteString(part)
		column += ansi.StringWidth(part)
	}
	flush()
	return out.String()
}
