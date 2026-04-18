// Package statusbar renders the persistent one-line status bar at the bottom.
package statusbar

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// Kind classifies the severity of a status message.
type Kind int

const (
	KindInfo Kind = iota
	KindWarn
	KindError
)

// Render returns a single-line status bar padded to width.
// msg is shown on the left; keys on the right (dimmed).
func Render(msg, keys string, kind Kind, width int) string {
	var left string
	switch kind {
	case KindWarn:
		left = styles.Warn.Render(msg)
	case KindError:
		left = styles.Err.Render(msg)
	default:
		left = styles.Muted.Render(msg)
	}

	right := styles.Key.Render(keys)
	gap := width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
	}
	return left + strings.Repeat(" ", gap) + right
}
