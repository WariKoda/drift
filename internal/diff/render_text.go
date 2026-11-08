package diff

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/rivo/uniseg"
)

func renderText(text string, spans []TextSpan, syntax []SyntaxSpan, base lipgloss.Style, options RenderOptions, width int) string {
	if !options.Highlight {
		spans = nil
	}
	if len(spans) == 0 && len(syntax) == 0 {
		text = string(expandTabs([]rune(text), 4))
		return ansi.Cut(text, max(0, options.Offset), max(0, options.Offset)+width)
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
	return ansi.Cut(out.String(), max(0, options.Offset), max(0, options.Offset)+width)
}
