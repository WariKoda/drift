package hostmanager

import (
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// noHit is the entry index returned when a click lands on nothing selectable.
const noHit = -1

// hitTest maps a terminal cell to an index into m.entries, or noHit.
//
// The list has no scroll offset — it always renders from entry 0 — but rows and
// entries are not one to one: a section header spans two rows. The walk below
// mirrors View's loop exactly, which is why both use entryRowSpan.
//
// Section headers are rendered like rows but are not selectable, so they report
// noHit: hitting a line and hitting a valid target are two different questions.
func (m Model) hitTest(x, y int) int {
	if y < headerLines {
		return noHit // title or separator
	}
	row := y - headerLines
	if row >= m.listHeight() {
		return noHit // bottom separator, status line, or past the viewport
	}

	consumed := 0
	for i, e := range m.entries {
		span := entryRowSpan(e)
		if consumed+span > m.listHeight() {
			break // View stopped here too
		}
		if row < consumed+span {
			if e.isHeader {
				return noHit
			}
			return i
		}
		consumed += span
	}
	return noHit // blank filler below the last entry
}

// updateMouse handles wheel and click events for the host list.
func (m Model) updateMouse(msg tea.MouseMsg) (Model, tea.Cmd) {
	// The delete prompt is modal: nothing behind it is clickable.
	if m.confirmDelete {
		return m, nil
	}

	if delta := mouse.WheelDelta(msg); delta != 0 {
		// No separate scroll offset exists here, so the wheel moves the cursor.
		m.statusMsg = ""
		m.cursor += delta
		if delta < 0 {
			m.clampCursorUp()
		} else {
			m.clampCursor()
		}
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

	// A double click opens the host for editing, matching Enter.
	if m.clicks.Register(msg.X, msg.Y) {
		e := m.currentEntry()
		if e == nil {
			return m, nil
		}
		h := e.host
		scope := e.scope
		return m, func() tea.Msg {
			return MsgOpenForm{Host: &h, Scope: scope, OldName: h.Name}
		}
	}

	return m, nil
}
