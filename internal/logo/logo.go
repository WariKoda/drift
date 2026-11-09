// Package logo contains the drift logo as terminal art. The SVG and PNG
// versions live in .github/assets/logo.
//
// Both variants use two characters per pixel, so the pixels stay square in a
// typical terminal font. Rows have different lengths; Lines pads them.
package logo

import "strings"

// Wordmark is the icon followed by "drift", 66 columns wide.
const Wordmark = `          ████████      ████████      ████    ████████    ████████
    ████  ████  ████    ████  ████    ████    ████          ████
          ████  ████    ████████      ████    ████████      ████
████████  ████  ████    ████████      ████    ████          ████
          ████████      ████  ████    ████    ████          ████`

// Icon is the mark alone, 20 columns wide and as tall as Wordmark.
const Icon = `          ████████
    ████  ████  ████
          ████  ████
████████  ████  ████
          ████████`

// Lines splits art into rows padded to the widest one, so centering each row
// keeps the picture intact.
func Lines(art string) []string {
	rows := strings.Split(art, "\n")
	width := 0
	for _, row := range rows {
		width = max(width, len([]rune(row)))
	}
	for i, row := range rows {
		rows[i] = row + strings.Repeat(" ", width-len([]rune(row)))
	}
	return rows
}
