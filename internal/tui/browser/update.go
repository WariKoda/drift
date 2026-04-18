package browser

import (
	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgSyncRequested is emitted when the user presses [s] with marked entries.
type MsgSyncRequested struct {
	Selection *fs.SelectionState
}

// MsgOpenHostManager is emitted when the user presses [H].
type MsgOpenHostManager struct{}

// Update handles key events and returns the updated model plus any command.
func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
		m.clampScroll()

	case tea.KeyMsg:
		// Filter mode captures most keys
		if m.filterMode {
			return m.updateFilter(msg)
		}
		return m.updateNormal(msg)
	}

	return m, nil
}

// updateNormal handles keys in normal (non-filter) mode.
func (m Model) updateNormal(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {

	// ── Quit ──────────────────────────────────────────
	case keyQ, keyCtrlC:
		return m, tea.Quit

	// ── Navigation ────────────────────────────────────
	case keyJ, keyDown:
		m.cursor++
		m.clampScroll()

	case keyK, keyUp:
		m.cursor--
		m.clampScroll()

	case keyG:
		m.cursor = 0
		m.clampScroll()

	case keyShiftG:
		m.cursor = len(m.entries) - 1
		m.clampScroll()

	// ── Expand / open ─────────────────────────────────
	case keyL, keyRight, keyEnter:
		if len(m.entries) == 0 {
			break
		}
		entry := m.entries[m.cursor]
		if entry.Kind == fs.EntryDir {
			if entry.Expanded {
				// already open — move cursor into first child
				if m.cursor+1 < len(m.entries) && m.entries[m.cursor+1].Depth > entry.Depth {
					m.cursor++
					m.clampScroll()
				}
			} else {
				if err := m.expandAt(m.cursor); err != nil {
					m.statusMsg = "Error: " + err.Error()
				}
				m.clampScroll()
			}
		}

	// ── Collapse / go to parent ────────────────────────
	case keyH, keyLeft:
		if len(m.entries) == 0 {
			break
		}
		entry := m.entries[m.cursor]
		if entry.Kind == fs.EntryDir && entry.Expanded {
			m.collapseAt(m.cursor)
			m.clampScroll()
		} else {
			p := m.parentIndex(m.cursor)
			if p >= 0 {
				m.collapseAt(p)
				m.cursor = p
				m.clampScroll()
			}
		}

	// ── Selection ─────────────────────────────────────
	case keySpace:
		if len(m.entries) == 0 {
			break
		}
		entry := m.entries[m.cursor]
		m.Selection.Toggle(entry.Path)

	case keyShiftV:
		// Mark all visible entries in the current depth level
		if len(m.entries) == 0 {
			break
		}
		depth := m.entries[m.cursor].Depth
		for _, e := range m.entries {
			if e.Depth == depth {
				m.Selection.Marked[e.Path] = struct{}{}
			}
		}

	case keyStar:
		// Invert selection
		for _, e := range m.entries {
			m.Selection.Toggle(e.Path)
		}

	case keyEsc:
		if m.filter != "" {
			m.filter = ""
		} else {
			m.Selection.Clear()
		}

	// ── Sync trigger ──────────────────────────────────
	case keyS:
		if m.Selection.Count() == 0 {
			m.statusMsg = "No files marked — use [Space] to mark files first"
			break
		}
		return m, func() tea.Msg {
			return MsgSyncRequested{Selection: m.Selection}
		}

	// ── Host Manager ───────────────────────────────────
	case "H":
		return m, func() tea.Msg { return MsgOpenHostManager{} }

	// ── Filter ────────────────────────────────────────
	case keySlash:
		m.filterMode = true
		m.filter = ""

	// ── Refresh ───────────────────────────────────────
	case keyR:
		if err := m.reload(); err != nil {
			m.statusMsg = "Refresh failed: " + err.Error()
		} else {
			m.statusMsg = "Refreshed"
		}

	// ── Help ──────────────────────────────────────────
	case keyQuestion:
		m.showHelp = !m.showHelp
	}

	return m, nil
}

// updateFilter handles key input while in filter mode.
func (m Model) updateFilter(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case keyEnter, keyEsc:
		m.filterMode = false
	case keyBackspace:
		if len(m.filter) > 0 {
			m.filter = m.filter[:len(m.filter)-1]
		}
	default:
		if len(msg.Runes) > 0 {
			m.filter += string(msg.Runes)
		}
	}
	// Reset cursor when filter changes
	m.cursor = 0
	m.offset = 0
	return m, nil
}
