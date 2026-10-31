package styles

import (
	"strconv"
	"strings"
	"unicode"
)

// SafeText escapes terminal controls without changing the underlying path or value.
func SafeText(value string) string {
	var out strings.Builder
	for _, r := range strings.ToValidUTF8(value, "�") {
		if unicode.IsControl(r) || unicode.Is(unicode.Bidi_Control, r) || r == '\u2028' || r == '\u2029' {
			quoted := strconv.QuoteRune(r)
			out.WriteString(quoted[1 : len(quoted)-1])
		} else {
			out.WriteRune(r)
		}
	}
	return out.String()
}
