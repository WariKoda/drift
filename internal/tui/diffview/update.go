package diffview

import (
	"fmt"

	tea "github.com/charmbracelet/bubbletea"
)

// MsgBackToBrowser is sent when the user quits the diff view.
type MsgBackToBrowser struct{}

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
		m.clampFileList()

	case tea.KeyMsg:
		return m.handleKey(msg)

	case MsgBulkSyncDone:
		m.syncing = false
		m.syncProgress = nil
		if len(msg.Errors) == 0 {
			m.syncStatus = fmt.Sprintf("✓ synced %d file(s)", msg.Done)
		} else {
			m.syncStatus = fmt.Sprintf("✓ %d  ✗ %d error(s)", msg.Done, len(msg.Errors))
		}
		// refresh diffs after sync
		m.refreshing = true
		return m, m.refreshCmd()

	case MsgSyncProgress:
		if !m.syncing || msg.Finished {
			return m, nil // sync finished; MsgBulkSyncDone owns the final state
		}
		m.syncDone = msg.Done
		m.syncTotal = msg.Total
		return m, syncProgressTickCmd(m.syncProgress)

	case MsgRefreshed:
		m.sessions = msg.Sessions
		m.syncDirs = make([]SyncDir, len(m.sessions))
		for i := range m.sessions {
			m.syncDirs[i] = autoDir(&m.sessions[i])
		}
		m.refreshing = false
		m.clampFileList()

	case MsgSynced:
		m.reloadSession(msg.SessionIdx)
		m.scroll = 0

	case MsgSyncError:
		if s := m.activeSession(); s != nil {
			s.Err = msg.Err
		}
	}
	return m, nil
}

// startBulkSync initializes the live progress tracker and kicks off the bulk
// sync alongside the periodic progress tick.
func (m Model) startBulkSync(indices []int) (Model, tea.Cmd) {
	m.syncing = true
	m.syncStatus = ""
	m.syncDone = 0
	m.syncTotal = len(indices)
	m.syncProgress = &LoadProgressTracker{}
	m.syncProgress.Set("Syncing…", 0, len(indices), false)
	return m, tea.Batch(m.bulkSyncCmd(indices), syncProgressTickCmd(m.syncProgress))
}

func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {

	// ── File list navigation ───────────────────────────────────────────
	case "j", "down", "tab":
		if m.activeIdx < len(m.sessions)-1 {
			m.activeIdx++
			m.scroll = 0
			m.clampFileList()
		}

	case "k", "up", "shift+tab":
		if m.activeIdx > 0 {
			m.activeIdx--
			m.scroll = 0
			m.clampFileList()
		}

	// ── Sync direction — current file (Space) or all files (A) ───────────
	case " ":
		if m.activeIdx >= 0 && m.activeIdx < len(m.sessions) {
			m.syncDirs[m.activeIdx] = nextDir(m.syncDirs[m.activeIdx], &m.sessions[m.activeIdx])
		}

	case "A":
		for i := range m.sessions {
			m.syncDirs[i] = nextDir(m.syncDirs[i], &m.sessions[i])
		}

	// ── Diff scroll ────────────────────────────────────────────────────
	case "J":
		m.scroll++
		m.clampScroll()

	case "K":
		m.scroll--
		m.clampScroll()

	case "ctrl+d":
		m.scroll += m.viewportHeight() / 2
		m.clampScroll()

	case "ctrl+u":
		m.scroll -= m.viewportHeight() / 2
		m.clampScroll()

	case "g":
		m.scroll = 0

	case "G":
		m.scroll = m.totalLines()
		m.clampScroll()

	// ── Jump to next/prev diff hunk ────────────────────────────────────
	case "]":
		m.jumpNextHunk()
	case "[":
		m.jumpPrevHunk()

	// ── Sync: current file with planned direction ──────────────────────
	case "s":
		if !m.syncing && !m.refreshing && m.activeIdx < len(m.syncDirs) {
			if m.syncDirs[m.activeIdx] != DirNone {
				return m.startBulkSync([]int{m.activeIdx})
			}
		}

	// ── Sync: all files with planned directions ────────────────────────
	case "S":
		if !m.syncing && !m.refreshing {
			indices := make([]int, len(m.sessions))
			for i := range indices {
				indices[i] = i
			}
			return m.startBulkSync(indices)
		}

	// ── Quick upload/download (bypass planned direction) ───────────────
	case "u":
		if s := m.activeSession(); s != nil && s.Result != nil && !s.Result.RemoteOnly {
			return m, m.uploadCmd(m.activeIdx)
		}

	case "d":
		if s := m.activeSession(); s != nil && s.Result != nil && !s.Result.LocalOnly {
			return m, m.downloadCmd(m.activeIdx)
		}

	// ── Refresh all diffs ──────────────────────────────────────────────
	case "r":
		if !m.refreshing {
			m.refreshing = true
			return m, m.refreshCmd()
		}

	// ── Quit ───────────────────────────────────────────────────────────
	case "q", "esc":
		return m, func() tea.Msg { return MsgBackToBrowser{} }
	}

	return m, nil
}

// jumpNextHunk moves scroll to the next diff hunk start.
func (m *Model) jumpNextHunk() {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return
	}
	lines := s.Result.Lines
	for i := m.scroll + 1; i < len(lines); i++ {
		if lines[i].Kind != 0 {
			m.scroll = i
			return
		}
	}
}

// jumpPrevHunk moves scroll to the previous diff hunk start.
func (m *Model) jumpPrevHunk() {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return
	}
	lines := s.Result.Lines
	for i := m.scroll - 1; i >= 0; i-- {
		if lines[i].Kind != 0 {
			m.scroll = i
			return
		}
	}
}
