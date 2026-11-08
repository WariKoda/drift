package diff

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/rivo/uniseg"
)

func renderText(text string, spans []TextSpan, base lipgloss.Style, options RenderOptions, width int) string {
	if len(spans) == 0 || !options.Highlight {
		text = string(expandTabs([]rune(text), 4))
		return ansi.Cut(text, max(0, options.Offset), max(0, options.Offset)+width)
	}
	var out, run strings.Builder
	column := 0
	marked := false
	flush := func() {
		if run.Len() == 0 {
			return
		}
		style := base
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
		for _, span := range spans {
			if span.Emphasis && start < span.End && end > span.Start {
				emphasis = true
				break
			}
		}
		if emphasis != marked {
			flush()
			marked = emphasis
		}
		part := g.Str()
		if part == "\r" {
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
	return ansi.Cut(out.String(), max(0, options.Offset), max(0, options.Offset)+width)
}
