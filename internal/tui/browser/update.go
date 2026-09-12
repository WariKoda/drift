package browser

import (
	"fmt"
	"os"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	"github.com/aymanbagabas/go-osc52/v2"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/muesli/termenv"
)

// MsgSyncRequested is emitted when the user presses [s] with marked entries.
// Host is set when the side-by-side remote browser already has an active host.
type MsgSyncRequested struct {
	Selection       *fs.SelectionState
	RemoteSelection *fs.SelectionState
	Host            *config.Host
	Conn            remote.Client
	Options         syncpolicy.ScopeOptions
}

// MsgOpenHostManager is emitted when the user presses [H].
type MsgOpenHostManager struct{}

// MsgBrowseRemoteRequested is emitted when the user wants to choose/change the
// host shown in the right-hand browser pane.
type MsgBrowseRemoteRequested struct{}

// MsgOpenDashboard is emitted when the user presses [P] to return to the
// project dashboard. The root app ignores it when no project registry is active.
type MsgOpenDashboard struct{}

type msgPreviewCopied struct {
	path string
	err  error
}

type msgClassifierReloaded struct {
	base       string
	classifier *fs.Classifier
	err        error
}

func (m Model) finishVisualSelection() Model {
	if !m.visualMode || m.activePane != m.visualPane {
		startPath := ""
		if m.activePane == PaneRemote {
			if entry := m.remoteCurrent(); entry != nil {
				startPath = entry.Path
			}
		} else if entries := m.filteredEntries(); m.cursor >= 0 && m.cursor < len(entries) {
			startPath = entries[m.cursor].Path
		}
		if startPath == "" {
			m.visualMode = false
			m.statusMsg = "No visible item to start a selection"
			return m
		}
		m.visualMode = true
		m.visualPane = m.activePane
		m.visualStartPath = startPath
		m.statusMsg = "Visual selection started; move and press [v] again"
		return m
	}

	entries := m.filteredEntries()
	cursor := m.cursor
	selection := m.Selection
	if m.visualPane == PaneRemote {
		entries = m.visibleRemoteEntries()
		cursor = m.remoteCursor
		selection = m.RemoteSelection
	}
	start := -1
	for i, entry := range entries {
		if entry.Path == m.visualStartPath {
			start = i
			break
		}
	}
	if start < 0 || cursor < 0 || cursor >= len(entries) {
		m.statusMsg = "Visual selection cancelled because its start is hidden"
		m.visualMode = false
		return m
	}
	if start > cursor {
		start, cursor = cursor, start
	}
	marked := 0
	for _, entry := range entries[start : cursor+1] {
		if m.visualPane == PaneRemote && entry.Unmapped {
			continue
		}
		selection.Marked[entry.Path] = struct{}{}
		marked++
	}
	m.visualMode = false
	m.statusMsg = fmt.Sprintf("Marked %d visible item(s)", marked)
	return m
}

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
		if !m.AcceptsRemoteResult(msg) {
			return m, func() tea.Msg {
				if msg.Conn != nil {
					if err := msg.Conn.Close(); err != nil {
						log.Error("close stale remote connection", "err", err)
					}
				}
				return nil
			}
		}
		m.applyRemoteLoaded(msg)
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

	case msgClassifierReloaded:
		if msg.base != m.WorkDir {
			return m, nil
		}
		if msg.err != nil {
			log.Error("browser classifier refresh failed", "root", msg.base, "err", msg.err)
			m.statusMsg = "Refresh failed: " + msg.err.Error()
			return m, nil
		}
		oldClassifier := m.classifier
		m.classifier = msg.classifier
		if err := m.reload(); err != nil {
			m.classifier = oldClassifier
			log.Error("browser refresh failed", "root", msg.base, "err", err)
			m.statusMsg = "Refresh failed: " + err.Error()
			return m, nil
		}
		m.statusMsg = "Refreshed"
		return m, m.schedulePreview()

	case msgPreviewCopied:
		if msg.err != nil {
			m.statusMsg = "Copy failed: " + sanitizePreviewError(msg.err)
			log.Error("copy preview failed", "path", msg.path, "err", msg.err)
		} else {
			m.statusMsg = "Copied preview to clipboard"
		}

	case msgFinderIndex:
		if m.finder.active && msg.base == m.WorkDir && msg.id == m.finder.id && msg.session == m.remoteSession {
			if msg.err != nil {
				log.Error("finder indexing failed", "root", msg.base, "err", msg.err)
				m.finder.loading = false
				m.finder.err = msg.err.Error()
				m.statusMsg = "Finder indexing failed: " + msg.err.Error()
				return m, nil
			}
			m.finder.rel = msg.rel
			m.finder.abs = msg.abs
			m.finder.ignored = msg.ignored
			m.finder.hidden = msg.hidden
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

	case keyC:
		if m.preview.active && m.preview.loaded {
			content := strings.Join(m.preview.lines, "\n")
			previewPath := m.preview.path
			return m, func() tea.Msg {
				sequence := osc52.New(content)
				if strings.HasPrefix(os.Getenv("TERM"), "screen") {
					sequence = sequence.Screen()
				}
				_, err := sequence.WriteTo(termenv.DefaultOutput())
				return msgPreviewCopied{path: previewPath, err: err}
			}
		}

	case keyPgUp, keyPgDown, keyHome, keyEnd:
		if m.preview.active {
			m.scrollPreview(msg.String())
		}

	// ── Pane focus ─────────────────────────────────────
	case keyTab:
		mouseCmd := m.disablePreview()
		if m.activePane == PaneLocal && m.remoteHost != nil {
			m.activePane = PaneRemote
		} else {
			m.activePane = PaneLocal
		}
		return m, mouseCmd

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
			m.remoteCursor = len(m.visibleRemoteEntries()) - 1
			m.clampRemoteScroll()
		} else {
			m.cursor = len(m.filteredEntries()) - 1
			m.clampScroll()
		}
		return m, m.schedulePreview()

	// ── Expand / open ─────────────────────────────────
	case keyL, keyRight, keyEnter:
		if m.activePane == PaneRemote {
			return m.updateRemoteOpen()
		}
		visible := m.filteredEntries()
		if m.cursor < 0 || m.cursor >= len(visible) {
			break
		}
		entry := visible[m.cursor]
		if entry.Kind == fs.EntryDir {
			if entry.Expanded {
				if m.cursor+1 < len(visible) && visible[m.cursor+1].Depth > entry.Depth {
					m.cursor++
					m.clampScroll()
					return m, m.schedulePreview()
				}
			} else if raw := m.localIndex(entry); raw >= 0 {
				if err := m.expandAt(raw); err != nil {
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
		visible := m.filteredEntries()
		if m.cursor < 0 || m.cursor >= len(visible) {
			break
		}
		entry := visible[m.cursor]
		if entry.Kind == fs.EntryDir && entry.Expanded {
			if raw := m.localIndex(entry); raw >= 0 {
				m.collapseAt(raw)
			}
			m.clampScroll()
		} else if entry.Parent != nil {
			parent := entry.Parent
			if raw := m.localIndex(parent); raw >= 0 {
				m.collapseAt(raw)
			}
			m.cursor = indexEntry(m.filteredEntries(), parent)
			m.clampScroll()
			return m, m.schedulePreview()
		}

	// ── Selection ─────────────────────────────────────
	case keyV:
		m = m.finishVisualSelection()

	case keySpace:
		if m.activePane == PaneRemote {
			if entry := m.remoteCurrent(); entry != nil {
				if entry.Unmapped {
					m.statusMsg = "Path is outside the active mappings"
				} else {
					m.RemoteSelection.Toggle(entry.Path)
				}
			}
			break
		}
		entries := m.filteredEntries()
		if m.cursor < 0 || m.cursor >= len(entries) {
			break
		}
		m.Selection.Toggle(entries[m.cursor].Path)

	case keyShiftV:
		if m.activePane == PaneRemote {
			entries := m.visibleRemoteEntries()
			if m.remoteCursor < 0 || m.remoteCursor >= len(entries) {
				break
			}
			parent := entries[m.remoteCursor].Parent
			for _, entry := range entries {
				if entry.Parent == parent && !entry.Unmapped {
					m.RemoteSelection.Marked[entry.Path] = struct{}{}
				}
			}
			break
		}
		entries := m.filteredEntries()
		if m.cursor < 0 || m.cursor >= len(entries) {
			break
		}
		parent := entries[m.cursor].Parent
		for _, entry := range entries {
			if entry.Parent == parent {
				m.Selection.Marked[entry.Path] = struct{}{}
			}
		}

	case keyStar:
		// Invert selection in the active pane.
		if m.activePane == PaneRemote {
			for _, entry := range m.visibleRemoteEntries() {
				if !entry.Unmapped {
					m.RemoteSelection.Toggle(entry.Path)
				}
			}
			break
		}
		for _, entry := range m.filteredEntries() {
			m.Selection.Toggle(entry.Path)
		}

	case keyEsc:
		if m.visualMode {
			m.visualMode = false
			m.statusMsg = "Visual selection cancelled"
			break
		}
		if m.filter != "" {
			m.filter = ""
			m.clampScroll()
			return m, m.schedulePreview()
		}
		m.Selection.Clear()
		m.RemoteSelection.Clear()

	// ── Sync trigger ──────────────────────────────────
	case keyS:
		if m.remoteConn != nil && m.remoteConn.Err() != nil {
			m.statusMsg = "Remote disconnected. Reconnect with [r] before comparing."
			break
		}
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		if m.Selection.Count()+m.RemoteSelection.Count() == 0 {
			m.statusMsg = "No files marked — use [Space] to mark files first"
			break
		}
		mouseCmd := m.disablePreview()
		var host *config.Host
		var conn remote.Client
		if m.remoteHost != nil {
			h := *m.remoteHost
			host = &h
			conn = m.remoteConn
			m.remoteConn = nil // hand connection ownership to the diff view
		}
		return m, tea.Batch(mouseCmd, func() tea.Msg {
			return MsgSyncRequested{Selection: m.Selection.Clone(), RemoteSelection: m.RemoteSelection.Clone(), Host: host, Conn: conn, Options: syncpolicy.ScopeOptions{}}
		})

	// ── Remote browser host ────────────────────────────
	case keyAt:
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		mouseCmd := m.disablePreview()
		return m, tea.Batch(mouseCmd, func() tea.Msg { return MsgBrowseRemoteRequested{} })

	// ── Host Manager ───────────────────────────────────
	case "H":
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		mouseCmd := m.disablePreview()
		return m, tea.Batch(mouseCmd, func() tea.Msg { return MsgOpenHostManager{} })

	// ── Project Dashboard ──────────────────────────────
	case "P":
		if m.remoteBusy() {
			m.statusMsg = "Wait for the remote operation to finish"
			break
		}
		mouseCmd := m.disablePreview()
		return m, tea.Batch(mouseCmd, func() tea.Msg { return MsgOpenDashboard{} })

	// ── Visibility ─────────────────────────────────────
	case keyDot, keyShiftI:
		var localCurrent, remoteCurrent *fs.FileEntry
		if entries := m.filteredEntries(); m.cursor >= 0 && m.cursor < len(entries) {
			localCurrent = entries[m.cursor]
		}
		remoteCurrent = m.remoteCurrent()
		if msg.String() == keyDot {
			m.showHidden = !m.showHidden
		} else {
			m.showIgnored = !m.showIgnored
		}
		if index := indexEntry(m.filteredEntries(), localCurrent); index >= 0 {
			m.cursor = index
		}
		if index := indexEntry(m.visibleRemoteEntries(), remoteCurrent); index >= 0 {
			m.remoteCursor = index
		}
		m.clampScroll()
		m.clampRemoteScroll()
		m.statusMsg = ""
		return m, m.schedulePreview()

	// ── Fuzzy file finder ──────────────────────────────
	case "f":
		mouseCmd := m.disablePreview()
		m.finderSeq++
		m.finder = finder{active: true, loading: true, id: m.finderSeq}
		return m, tea.Batch(mouseCmd, buildFinderIndexCmd(m.WorkDir, m.classifier, m.showHidden, m.showIgnored, m.finder.id, m.remoteSession))

	// ── Filter ────────────────────────────────────────
	case keySlash:
		mouseCmd := m.disablePreview()
		m.filterMode = true
		m.filter = ""
		return m, mouseCmd

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
		base := m.WorkDir
		previewCmd := m.schedulePreview()
		classifierCmd := func() tea.Msg {
			classifier, err := fs.NewClassifier(base)
			return msgClassifierReloaded{base: base, classifier: classifier, err: err}
		}
		return m, tea.Batch(previewCmd, classifierCmd)

	// ── Help ──────────────────────────────────────────
	case keyQuestion:
		mouseCmd := m.disablePreview()
		m.showHelp = !m.showHelp
		return m, mouseCmd
	}

	return m, nil
}

func (m Model) updateRemoteOpen() (Model, tea.Cmd) {
	entries := m.visibleRemoteEntries()
	if m.remoteBusy() || m.remoteConn == nil || m.remoteConn.Err() != nil || m.remoteCursor < 0 || m.remoteCursor >= len(entries) {
		return m, nil
	}
	entry := entries[m.remoteCursor]
	if entry.Kind != fs.EntryDir {
		return m, nil
	}
	if entry.Expanded {
		if m.remoteCursor+1 < len(entries) && entries[m.remoteCursor+1].Depth > entry.Depth {
			m.remoteCursor++
			m.clampRemoteScroll()
		}
		return m, m.schedulePreview()
	}
	entry.Expanded = true
	m.remoteReading = true
	m.remoteStatus = "Loading remote: " + entry.Path
	return m, readRemoteDirCmd(m.remoteConn, *m.remoteHost, m.remoteLoadID, m.remoteSession, entry.Path, m.classifier, m.config, m.WorkDir)
}

func (m Model) updateRemoteClose() (Model, tea.Cmd) {
	entry := m.remoteCurrent()
	if entry == nil {
		return m, nil
	}
	if entry.Kind == fs.EntryDir && entry.Expanded {
		if raw := m.remoteIndexByPath(entry.Path); raw >= 0 {
			m.collapseRemoteAt(raw)
		}
		m.clampRemoteScroll()
		return m, nil
	}
	if entry.Parent != nil {
		parent := entry.Parent
		if raw := m.remoteIndexByPath(parent.Path); raw >= 0 {
			m.collapseRemoteAt(raw)
		}
		m.remoteCursor = indexEntry(m.visibleRemoteEntries(), parent)
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
