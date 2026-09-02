package diffview

import (
	"fmt"

	"github.com/WariKoda/drift/internal/log"
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

	case tea.MouseMsg:
		return m.updateMouse(msg)

	case tea.KeyMsg:
		return m.handleKey(msg)

	case MsgBulkSyncDone:
		m.syncing = false
		m.syncProgress = nil
		m.syncErrors = msg.Errors
		log.Info("bulk sync done", "done", msg.Done, "errors", len(msg.Errors))
		if len(msg.Errors) == 0 {
			m.syncStatus = fmt.Sprintf("✓ synced %d file(s)", msg.Done)
			m.showErrors = false
		} else {
			m.syncStatus = fmt.Sprintf("✓ %d  ✗ %d error(s) — [e] to view", msg.Done, len(msg.Errors))
			m.showErrors = true
		}
		// Refresh diffs as the final phase of the same global activity.
		m.refreshing = true
		m.activityLabel = "Refreshing diffs…"
		m.activityTracker.Set(m.activityLabel, 0, len(m.sessions), len(m.sessions) == 0)
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
		m.finishActivity()
		m.clampFileList()
		m.scrollToFirstDifference()

	case MsgSynced:
		m.activityLabel = "Refreshing diff…"
		m.activityTracker.Set(m.activityLabel, 0, 1, false)
		return m, m.reloadSessionCmd(msg.SessionIdx)

	case MsgSessionReloaded:
		reloadedActiveSession := msg.SessionIdx == m.activeIdx
		if msg.SessionIdx >= 0 && msg.SessionIdx < len(m.sessions) {
			m.sessions[msg.SessionIdx].Result = msg.Result
			m.sessions[msg.SessionIdx].Err = msg.Err
			if reloadedActiveSession {
				m.scrollToFirstDifference()
			}
		}
		m.quickSyncing = false
		m.finishActivity()

	case MsgSyncError:
		m.quickSyncing = false
		m.finishActivity()
		log.Error("sync error", "err", msg.Err)
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
	m.syncProgress = m.beginActivity("Syncing files…", len(indices))
	return m, tea.Batch(m.bulkSyncCmd(indices), syncProgressTickCmd(m.syncProgress))
}

func (m Model) remoteBusy() bool {
	return m.syncing || m.quickSyncing || m.refreshing
}

func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {

	// ── File list navigation ───────────────────────────────────────────
	case "tab", "n":
		if m.activeIdx < len(m.sessions)-1 {
			m.activeIdx++
			m.clampFileList()
			m.scrollToFirstDifference()
		}

	case "shift+tab", "p":
		if m.activeIdx > 0 {
			m.activeIdx--
			m.clampFileList()
			m.scrollToFirstDifference()
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
	case "j", "down", "J":
		m.scroll++
		m.clampScroll()

	case "k", "up", "K":
		m.scroll--
		m.clampScroll()

	case "ctrl+d":
		m.scroll += max(1, m.viewportHeight()/2)
		m.clampScroll()

	case "ctrl+u":
		m.scroll -= max(1, m.viewportHeight()/2)
		m.clampScroll()

	case "pgdown":
		m.scroll += m.viewportHeight()
		m.clampScroll()

	case "pgup":
		m.scroll -= m.viewportHeight()
		m.clampScroll()

	case "home", "g":
		m.scroll = 0

	case "end", "G":
		m.scroll = m.totalLines()
		m.clampScroll()

	// ── Jump to next/prev diff hunk ────────────────────────────────────
	case "]":
		m.jumpNextHunk()
	case "[":
		m.jumpPrevHunk()

	// ── Sync: current file with planned direction ──────────────────────
	case "s":
		if !m.remoteBusy() && m.activeIdx < len(m.syncDirs) {
			if m.syncDirs[m.activeIdx] != DirNone {
				return m.startBulkSync([]int{m.activeIdx})
			}
		}

	// ── Sync: all files with planned directions ────────────────────────
	case "S":
		if !m.remoteBusy() {
			indices := make([]int, len(m.sessions))
			for i := range indices {
				indices[i] = i
			}
			return m.startBulkSync(indices)
		}

	// ── Quick upload/download (bypass planned direction) ───────────────
	case "u":
		if s := m.activeSession(); !m.remoteBusy() && s != nil && s.Result != nil && !s.Result.RemoteOnly {
			m.quickSyncing = true
			m.beginActivity("Uploading file…", 0)
			return m, m.uploadCmd(m.activeIdx)
		}

	case "d":
		if s := m.activeSession(); !m.remoteBusy() && s != nil && s.Result != nil && !s.Result.LocalOnly {
			m.quickSyncing = true
			m.beginActivity("Downloading file…", 0)
			return m, m.downloadCmd(m.activeIdx)
		}

	// ── Refresh all diffs ──────────────────────────────────────────────
	case "r":
		if !m.remoteBusy() {
			m.refreshing = true
			m.beginActivity("Refreshing diffs…", len(m.sessions))
			return m, m.refreshCmd()
		}

	// ── Toggle the bulk-sync error overlay ─────────────────────────────
	case "e":
		if len(m.syncErrors) > 0 {
			m.showErrors = !m.showErrors
		}

	// ── Quit ───────────────────────────────────────────────────────────
	case "q", "esc":
		if m.showErrors {
			m.showErrors = false
			return m, nil
		}
		if m.remoteBusy() {
			return m, nil
		}
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
			m.clampScroll()
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
			m.clampScroll()
			return
		}
	}
}
