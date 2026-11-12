package styles

import (
	"regexp"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// Include bracket keys themselves: [[] and []]. Call only for hint text, not
// file names, field values, or progress bars that also contain brackets.
var keyHintPattern = regexp.MustCompile(`\[(?:\[|\]|[^\[\]\r\n]+)\]`)

// KeyHints renders bracketed keys in the directory/primary style, keeping
// descriptions in base. text must be plain text, without embedded ANSI styles.
func KeyHints(text string, base lipgloss.Style) string {
	text = SafeText(text)
	var out strings.Builder
	start := 0
	for _, match := range keyHintPattern.FindAllStringIndex(text, -1) {
		out.WriteString(base.Render(text[start:match[0]]))
		out.WriteString(Dir.Render(text[match[0]:match[1]]))
		start = match[1]
	}
	out.WriteString(base.Render(text[start:]))
	return out.String()
}

// FitKeyHints keeps whole hints in priority order, reserving the final hint
// for help or the action that closes the current mode.
func FitKeyHints(width int, hints ...string) string {
	if len(hints) == 0 {
		return ""
	}
	last := hints[len(hints)-1]
	if lipgloss.Width(last) > width {
		return ""
	}
	remaining := width - lipgloss.Width(last)
	var selected []string
	for _, hint := range hints[:len(hints)-1] {
		needed := lipgloss.Width(hint) + 2
		if needed <= remaining {
			selected = append(selected, hint)
			remaining -= needed
		}
	}
	return strings.Join(append(selected, last), "  ")
}
