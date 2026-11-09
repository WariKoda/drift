package logo

import (
	"testing"
	"unicode/utf8"
)

func TestLinesPadsToEqualWidth(t *testing.T) {
	for name, tc := range map[string]struct {
		art   string
		width int
	}{
		"wordmark": {Wordmark, 66},
		"icon":     {Icon, 20},
	} {
		rows := Lines(tc.art)
		if len(rows) != 5 {
			t.Fatalf("%s: %d rows, want 5", name, len(rows))
		}
		for i, row := range rows {
			if got := utf8.RuneCountInString(row); got != tc.width {
				t.Errorf("%s row %d: width %d, want %d", name, i, got, tc.width)
			}
		}
	}
}
