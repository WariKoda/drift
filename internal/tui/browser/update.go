package browser

import (
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgSyncRequested is emitted when the user presses [s] with marked entries.
// Host is set when the side-by-side remote browser already has an active host.
type MsgSyncRequested struct {
	Selection *fs.SelectionState
	Host      *config.Host
}

// MsgOpenHostManager is emitted when the user presses [H].
type MsgOpenHostManager struct{}

// MsgBrowseRemoteRequested is emitted when the user wants to choose/change the
// host shown in the right-hand browser pane.
type MsgBrowseRemoteRequested struct{}

// MsgOpenDashboard is emitted when the user presses [P] to return to the
// project dashboard. The root app ignores it when no project registry is active.
type MsgOpenDashboard struct{}

// Update handles key events and returns the updated model plus any command.
func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
		m.clampScroll()
		m.clampRemoteScroll()

	case MsgRemoteLoaded:
		m.applyRemoteLoaded(msg)

	case MsgRemoteChildrenLoaded:
		m.applyRemoteChildrenLoaded(msg)

	case msgFinderIndex:
		if m.finder.active && msg.base == m.WorkDir {
			m.finder.rel = msg.rel
			m.finder.abs = msg.abs
			m.finder.loading = false
			m.finder.recompute()
			m.finder.clamp(m.finderViewportHeight())
		}

	case tea.KeyMsg:
		// Overlays capture keys first.
		if m.finder.active {
			return m.updateFinder(msg)
		}
		if m.filterMode {
			return m.updateFilter(msg)
		}
		return m.updateNormal(msg)
	}

	return m, nil
}

// updateFinder handles keys while the fuzzy file finder is open.
func (m Model) updateFinder(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case "esc", "enter":
		m.finder.active = false

	case "down", "ctrl+n":
		m.finder.cursor++
		m.finder.clamp(m.finderViewportHeight())

	case "up", "ctrl+p":
		m.finder.cursor--
		m.finder.clamp(m.finderViewportHeight())

	case " ":
		if r := m.finder.current(); r != nil {
			m.Selection.Toggle(r.abs)
		}

	case "ctrl+u":
		m.finder.query = ""
		m.finder.recompute()
		m.finder.clamp(m.finderViewportHeight())

	case "backspace", "ctrl+h":
		if rq := []rune(m.finder.query); len(rq) > 0 {
			m.finder.query = string(rq[:len(rq)-1])
			m.finder.recompute()
			m.finder.clamp(m.finderViewportHeight())
		}

	default:
		if len(msg.Runes) > 0 {
			m.finder.query += string(msg.Runes)
			m.finder.recompute()
			m.finder.clamp(m.finderViewportHeight())
		}
	}
	return m, nil
}

// updateNormal handles keys in normal (non-filter) mode.
func (m Model) updateNormal(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {

	// ── Quit ──────────────────────────────────────────
	case keyQ, keyCtrlC:
		return m, tea.Quit

	// ── Pane focus ─────────────────────────────────────
	case keyTab:
		if m.activePane == PaneLocal && m.remoteHost != nil {
			m.activePane = PaneRemote
		} else {
			m.activePane = PaneLocal
		}

	// ── Navigation ────────────────────────────────────
	case keyJ, keyDown:
		if m.activePane == PaneRemote {
			m.remoteCursor++
			m.clampRemoteScroll()
		} else {
			m.cursor++
			m.clampScroll()
		}

	case keyK, keyUp:
		if m.activePane == PaneRemote {
			m.remoteCursor--
			m.clampRemoteScroll()
		} else {
			m.cursor--
			m.clampScroll()
		}

	case keyG:
		if m.activePane == PaneRemote {
			m.remoteCursor = 0
			m.clampRemoteScroll()
		} else {
			m.cursor = 0
			m.clampScroll()
		}

	case keyShiftG:
		if m.activePane == PaneRemote {
			m.remoteCursor = len(m.remoteEntries) - 1
			m.clampRemoteScroll()
		} else {
			m.cursor = len(m.entries) - 1
			m.clampScroll()
		}

	// ── Expand / open ─────────────────────────────────
	case keyL, keyRight, keyEnter:
		if m.activePane == PaneRemote {
			return m.updateRemoteOpen()
		}
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
		if m.activePane == PaneRemote {
			return m.updateRemoteClose()
		}
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
		if m.activePane == PaneRemote {
			break
		}
		if len(m.entries) == 0 {
			break
		}
		entry := m.entries[m.cursor]
		m.Selection.Toggle(entry.Path)

	case keyShiftV:
		if m.activePane == PaneRemote {
			break
		}
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
		if m.activePane == PaneRemote {
			break
		}
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
		var host *config.Host
		if m.remoteHost != nil {
			h := *m.remoteHost
			host = &h
		}
		return m, func() tea.Msg {
			return MsgSyncRequested{Selection: m.Selection, Host: host}
		}

	// ── Remote browser host ────────────────────────────
	case keyAt:
		return m, func() tea.Msg { return MsgBrowseRemoteRequested{} }

	// ── Host Manager ───────────────────────────────────
	case "H":
		return m, func() tea.Msg { return MsgOpenHostManager{} }

	// ── Project Dashboard ──────────────────────────────
	case "P":
		return m, func() tea.Msg { return MsgOpenDashboard{} }

	// ── Fuzzy file finder ──────────────────────────────
	case "f":
		m.finder = finder{active: true, loading: true}
		return m, buildFinderIndexCmd(m.WorkDir)

	// ── Filter ────────────────────────────────────────
	case keySlash:
		m.filterMode = true
		m.filter = ""

	// ── Refresh ───────────────────────────────────────
	case keyR:
		if m.activePane == PaneRemote && m.remoteHost != nil {
			h := *m.remoteHost
			cmd := m.StartRemote(h)
			return m, cmd
		}
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

func (m Model) updateRemoteOpen() (Model, tea.Cmd) {
	if m.remoteLoading || m.remoteConn == nil || len(m.remoteEntries) == 0 {
		return m, nil
	}
	entry := m.remoteEntries[m.remoteCursor]
	if entry.Kind != fs.EntryDir {
		return m, nil
	}
	if entry.Expanded {
		if m.remoteCursor+1 < len(m.remoteEntries) && m.remoteEntries[m.remoteCursor+1].Depth > entry.Depth {
			m.remoteCursor++
			m.clampRemoteScroll()
		}
		return m, nil
	}
	entry.Expanded = true // optimistic spinner/guard against duplicate expand
	m.remoteStatus = "Loading remote: " + entry.Path
	return m, readRemoteDirCmd(m.remoteConn, entry.Path)
}

func (m Model) updateRemoteClose() (Model, tea.Cmd) {
	if len(m.remoteEntries) == 0 {
		return m, nil
	}
	entry := m.remoteEntries[m.remoteCursor]
	if entry.Kind == fs.EntryDir && entry.Expanded {
		m.collapseRemoteAt(m.remoteCursor)
		m.clampRemoteScroll()
		return m, nil
	}
	p := m.remoteParentIndex(m.remoteCursor)
	if p >= 0 {
		m.collapseRemoteAt(p)
		m.remoteCursor = p
		m.clampRemoteScroll()
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
