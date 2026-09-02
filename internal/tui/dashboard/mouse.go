package dashboard

import (
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// noHit is the entry index returned when a click lands on nothing selectable.
const noHit = -1

// hitTest maps a terminal cell to an index into m.entries, or noHit.
//
// The list windows around the cursor (see windowStart) rather than scrolling
// with a separate offset. Rows are the centered block — clicks on the logo,
// side padding, blank filler, or footer are ignored.
func (m Model) hitTest(x, y int) int {
	rel := y - m.listTop()
	if rel < 0 {
		return noHit // logo or top padding
	}

	start := m.windowStart()
	visible := len(m.entries) - start
	if visible > m.listMax() {
		visible = m.listMax()
	}
	if rel >= visible {
		return noHit // blank filler, footer, or past the last entry
	}

	left := m.blockLeft()
	if x < left || x >= left+m.blockWidth() {
		return noHit // outside the centered block
	}
	return start + rel
}

// updateMouse handles wheel and click events for the project list.
func (m Model) updateMouse(msg tea.MouseMsg) (Model, tea.Cmd) {
	// The delete prompt is modal: nothing behind it is clickable.
	if m.confirmDelete {
		return m, nil
	}

	if delta := mouse.WheelDelta(msg); delta != 0 {
		m.statusMsg = ""
		m.cursor += delta
		m.clampCursor()
		return m, nil
	}

	if !mouse.IsLeftPress(msg) {
		return m, nil
	}

	idx := m.hitTest(msg.X, msg.Y)
	if idx == noHit {
		return m, nil
	}

	m.statusMsg = ""
	m.cursor = idx

	if m.clicks.Register(msg.X, msg.Y) {
		return m.chooseCurrent()
	}
	return m, nil
}
