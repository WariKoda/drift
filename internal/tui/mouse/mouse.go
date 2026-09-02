// Package mouse holds the mouse helpers shared by the TUI screens.
//
// Screens are sub-packages of internal/tui and cannot import their parent, so
// the pieces every screen needs — wheel classification, the scroll step, and
// double-click detection — live here.
package mouse

import (
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

// ScrollStep is how many lines one wheel notch moves the viewport.
// Three lines is the usual terminal convention.
const ScrollStep = 3

// DoubleClickWindow is how long after a click a second click on the same cell
// still counts as a double click.
const DoubleClickWindow = 400 * time.Millisecond

// IsWheel reports whether the event came from the scroll wheel.
func IsWheel(m tea.MouseMsg) bool {
	return tea.MouseEvent(m).IsWheel()
}

// WheelDelta returns the number of lines to scroll: negative for up, positive
// for down, and zero for anything that is not a vertical wheel event.
// Horizontal wheel events return zero — no screen scrolls sideways.
func WheelDelta(m tea.MouseMsg) int {
	switch m.Button {
	case tea.MouseButtonWheelUp:
		return -ScrollStep
	case tea.MouseButtonWheelDown:
		return ScrollStep
	default:
		return 0
	}
}

// IsLeftPress reports whether the event is the left button going down.
// Screens act on the press, not the release, so a click registers immediately.
func IsLeftPress(m tea.MouseMsg) bool {
	return m.Action == tea.MouseActionPress && m.Button == tea.MouseButtonLeft
}

// ClickTracker turns a stream of clicks into single and double clicks.
// Bubble Tea reports every press separately; it has no double-click event.
//
// The zero value is ready to use. Embed it in a screen model by value: it is
// copied along with the model, which is what Bubble Tea's value-receiver
// Update contract needs.
type ClickTracker struct {
	lastX, lastY int
	lastAt       time.Time
	// primed guards against a triple click counting as two double clicks:
	// after one fires, the next press starts over.
	primed bool
}

// Register records a click at the given cell and reports whether it completes
// a double click — a second press on the same cell within DoubleClickWindow.
func (t *ClickTracker) Register(x, y int) bool {
	return t.registerAt(x, y, time.Now())
}

// registerAt is Register with an injectable clock, so tests need no sleeps.
func (t *ClickTracker) registerAt(x, y int, now time.Time) bool {
	double := t.primed &&
		x == t.lastX && y == t.lastY &&
		now.Sub(t.lastAt) <= DoubleClickWindow

	t.lastX, t.lastY = x, y
	t.lastAt = now
	t.primed = !double // a completed double click resets the sequence

	return double
}
