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
