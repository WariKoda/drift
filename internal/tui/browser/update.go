package browser

import (
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgSyncRequested is emitted when the user presses [s] with marked entries.
// Host is set when the side-by-side remote browser already has an active host.
type MsgSyncRequested struct {
	Selection       *fs.SelectionState
	RemoteSelection *fs.SelectionState
	Host            *config.Host
	Conn            remote.Client
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
		m.layoutPreview(true)

	case MsgRemoteLoaded:
		stale := m.remoteHost == nil || m.remoteHost.Name != msg.Host.Name
		m.applyRemoteLoaded(msg)
		if stale {
			return m, nil
		}
		if msg.Err != nil {
			m.preview.loading = false
			m.preview.waiting = false
			if m.preview.active && !m.preview.loaded && m.preview.source == PaneRemote {
				m.preview.message = "Preview unavailable: " + sanitizePreviewError(msg.Err)
			}
			return m, nil
		}
		if cmd := m.resumePreviewLoad(); cmd != nil {
			return m, cmd
		}
		return m, m.schedulePreview()

	case MsgRemoteChildrenLoaded:
		m.applyRemoteChildrenLoaded(msg)
		return m, m.resumePreviewLoad()

	case msgPreviewDebounced:
		return m, m.beginPreviewLoad(msg.request)

	case msgPreviewLoaded:
		return m, m.applyPreviewLoaded(msg)

	case msgFinderIndex:
		if m.finder.active && msg.base == m.WorkDir {
			m.finder.rel = msg.rel
			m.finder.abs = msg.abs
			m.finder.loading = false
			m.finder.recompute()
			m.finder.clamp(m.finderViewportHeight())
		}

	case tea.MouseMsg:
		return m.updateMouse(msg)

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

	// ── File preview ───────────────────────────────────
	case keyP:
		return m, m.togglePreview()

	case keyPgUp, keyPgDown, keyHome, keyEnd:
		if m.preview.active {
			m.scrollPreview(msg.String())
		}

	// ── Pane focus ─────────────────────────────────────
	case keyTab:
		m.disablePreview()
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
		return m, m.schedulePreview()

	case keyK, keyUp:
		if m.activePane == PaneRemote {
			m.remoteCursor--
			m.clampRemoteScroll()
		} else {
			m.cursor--
			m.clampScroll()
		}
		return m, m.schedulePreview()

	case keyG:
		if m.activePane == PaneRemote {
			m.remoteCursor = 0
			m.clampRemoteScroll()
		} else {
			m.cursor = 0
			m.clampScroll()
		}
		return m, m.schedulePreview()

	case keyShiftG:
		if m.activePane == PaneRemote {
			m.remoteCursor = len(m.remoteEntries) - 1
			m.clampRemoteScroll()
		} else {
			m.cursor = len(m.entries) - 1
			m.clampScroll()
		}
		return m, m.schedulePreview()

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
					return m, m.schedulePreview()
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
				return m, m.schedulePreview()
			}
		}

	// ── Selection ─────────────────────────────────────
	case keySpace:
		if m.activePane == PaneRemote {
			if entry := m.remoteCurrent(); entry != nil {
				m.RemoteSelection.Toggle(entry.Path)
			}
			break
		}
		if len(m.entries) == 0 {
			break
		}
		entry := m.entries[m.cursor]
		m.Selection.Toggle(entry.Path)

	case keyShiftV:
		// Mark all visible entries in the current depth level of the active pane.
		if m.activePane == PaneRemote {
			if len(m.remoteEntries) == 0 {
				break
			}
			depth := m.remoteEntries[m.remoteCursor].Depth
			for _, e := range m.remoteEntries {
				if e.Depth == depth {
					m.RemoteSelection.Marked[e.Path] = struct{}{}
				}
			}
			break
		}
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
		// Invert selection in the active pane.
		if m.activePane == PaneRemote {
			for _, e := range m.remoteEntries {
				m.RemoteSelection.Toggle(e.Path)
			}
			break
		}
		for _, e := range m.entries {
			m.Selection.Toggle(e.Path)
		}

	case keyEsc:
		if m.filter != "" {
			m.filter = ""
			m.clampScroll()
			return m, m.schedulePreview()
		}
		m.Selection.Clear()
		m.RemoteSelection.Clear()

	// ── Sync trigger ──────────────────────────────────
	case keyS:
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		if m.Selection.Count()+m.RemoteSelection.Count() == 0 {
			m.statusMsg = "No files marked — use [Space] to mark files first"
			break
		}
		m.disablePreview()
		var host *config.Host
		var conn remote.Client
		if m.remoteHost != nil {
			h := *m.remoteHost
			host = &h
			conn = m.remoteConn
			m.remoteConn = nil // hand connection ownership to the diff view
		}
		return m, func() tea.Msg {
			return MsgSyncRequested{Selection: m.Selection, RemoteSelection: m.RemoteSelection, Host: host, Conn: conn}
		}

	// ── Remote browser host ────────────────────────────
	case keyAt:
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		m.disablePreview()
		return m, func() tea.Msg { return MsgBrowseRemoteRequested{} }

	// ── Host Manager ───────────────────────────────────
	case "H":
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		m.disablePreview()
		return m, func() tea.Msg { return MsgOpenHostManager{} }

	// ── Project Dashboard ──────────────────────────────
	case "P":
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		m.disablePreview()
		return m, func() tea.Msg { return MsgOpenDashboard{} }

	// ── Fuzzy file finder ──────────────────────────────
	case "f":
		m.disablePreview()
		m.finder = finder{active: true, loading: true}
		return m, buildFinderIndexCmd(m.WorkDir)

	// ── Filter ────────────────────────────────────────
	case keySlash:
		m.disablePreview()
		m.filterMode = true
		m.filter = ""

	// ── Refresh ───────────────────────────────────────
	case keyR:
		if m.activePane == PaneRemote && m.remoteHost != nil {
			if m.remoteBusy() {
				m.statusMsg = "Wait for the remote operation to finish"
				break
			}
			m.prepareRemotePreviewRefresh()
			h := *m.remoteHost
			return m, m.StartRemote(h)
		}
		if err := m.reload(); err != nil {
			m.statusMsg = "Refresh failed: " + err.Error()
		} else {
			m.statusMsg = "Refreshed"
			return m, m.schedulePreview()
		}

	// ── Help ──────────────────────────────────────────
	case keyQuestion:
		m.disablePreview()
		m.showHelp = !m.showHelp
	}

	return m, nil
}

func (m Model) updateRemoteOpen() (Model, tea.Cmd) {
	if m.remoteBusy() || m.remoteConn == nil || len(m.remoteEntries) == 0 {
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
		return m, m.schedulePreview()
	}
	entry.Expanded = true // optimistic spinner/guard against duplicate expand
	m.remoteReading = true
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
		return m, m.schedulePreview()
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
