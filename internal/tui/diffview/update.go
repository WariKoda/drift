package diffview

import (
	"errors"
	"fmt"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgBackToBrowser is sent when the user quits the diff view.
type MsgBackToBrowser struct{}

func (m Model) Update(msg tea.Msg) (next Model, cmd tea.Cmd) {
	defer func() {
		next.prepareDisplayRows()
		switch msg.(type) {
		case tea.KeyMsg, tea.MouseMsg:
		default:
			return
		}
		if extra := next.startDecoration(); extra != nil {
			if cmd == nil {
				cmd = extra
			} else {
				cmd = tea.Batch(cmd, extra)
			}
		}
	}()
	if m.closed {
		return m, nil
	}
	if m.conn != nil {
		m.ConnectionLost(m.conn, m.conn.Err())
	}
	switch msg := msg.(type) {

	case MsgDecorated:
		m.acceptDecoration(msg)
		return m, m.startDecoration()

	case tea.WindowSizeMsg:
		m.SetSize(msg.Width, msg.Height)

	case tea.MouseMsg:
		return m.updateMouse(msg)

	case tea.KeyMsg:
		return m.handleKey(msg)

	case MsgBulkSyncDone:
		if msg.Conn != nil && msg.Conn != m.conn {
			return m, nil
		}
		m.ConnectionLost(msg.Conn, msg.Err)
		if m.completed == nil {
			m.completed = make(map[int]bool)
		}
		var deletedLocal, deletedRemote []string
		for _, idx := range msg.Completed {
			if idx >= 0 && idx < len(m.syncDirs) {
				if idx < len(m.sessions) {
					switch m.syncDirs[idx] {
					case DirDeleteLocal:
						deletedLocal = append(deletedLocal, m.sessions[idx].LocalPath)
					case DirDeleteRemote:
						deletedRemote = append(deletedRemote, m.sessions[idx].RemotePath)
					}
				}
				m.completed[idx] = true
				m.syncDirs[idx] = DirNone
			}
		}
		canceled := m.activityTracker.Canceled()
		m.syncing = false
		m.syncProgress = nil
		m.syncErrors = msg.Errors
		m.errorOffset = 0
		log.Info("bulk sync done", "done", msg.Done, "errors", len(msg.Errors), "cancelled", canceled)
		if m.connectionError() != nil {
			m.syncStatus = fmt.Sprintf("stopped: ✓ %d  ✗ %d", msg.Done, len(msg.Errors))
			m.showErrors = len(msg.Errors) > 0
			m.finishActivity()
			return m, nil
		}
		for _, failure := range msg.Errors {
			var verificationErr *tlstrust.VerificationError
			if failure.Err != nil && errors.As(failure.Err, &verificationErr) {
				m.syncStatus = fmt.Sprintf("stopped after certificate error — ✓ %d  ✗ %d", msg.Done, len(msg.Errors))
				m.showErrors = true
				m.finishActivity()
				return m, nil
			}
		}
		if canceled {
			if len(msg.Errors) == 0 {
				m.syncStatus = fmt.Sprintf("cancelled — %d file(s) synced", msg.Done)
				m.showErrors = false
			} else {
				m.syncStatus = fmt.Sprintf("cancelled — ✓ %d  ✗ %d — [e] to view", msg.Done, len(msg.Errors))
				m.showErrors = true
			}
			m.finishActivity()
			return m, nil
		}
		if len(msg.Errors) == 0 {
			m.syncStatus = fmt.Sprintf("✓ synced %d file(s)", msg.Done)
			m.showErrors = false
		} else {
			m.syncStatus = fmt.Sprintf("✓ %d  ✗ %d error(s) — [e] to view", msg.Done, len(msg.Errors))
			m.showErrors = true
		}
		m.finishActivity()
		status := m.syncStatus
		failures := append([]syncpolicy.Failure(nil), m.syncErrors...)
		return m, func() tea.Msg {
			return MsgScopeReloadRequested{
				IncludeIgnored: m.scopeOptions.IncludeIgnored,
				Status:         status,
				Errors:         failures,
				DeletedLocal:   deletedLocal,
				DeletedRemote:  deletedRemote,
			}
		}

	case MsgSyncProgress:
		if !m.syncing || msg.Finished {
			return m, nil // sync finished; MsgBulkSyncDone owns the final state
		}
		m.syncDone = msg.Done
		m.syncTotal = msg.Total
		return m, syncProgressTickCmd(m.syncProgress)

	case MsgSynced:
		if msg.Conn != nil && msg.Conn != m.conn {
			return m, nil
		}
		if msg.SessionIdx < 0 || msg.SessionIdx >= len(m.sessions) {
			return m, nil
		}
		if m.completed == nil {
			m.completed = make(map[int]bool)
		}
		m.completed[msg.SessionIdx] = true
		m.syncDirs[msg.SessionIdx] = DirNone
		m.syncStatus = "✓ synced 1 file"
		if m.connectionError() != nil || m.activityTracker.Canceled() {
			m.quickSyncing = false
			m.finishActivity()
			return m, nil
		}
		m.quickSyncing = false
		m.finishActivity()
		status := m.syncStatus
		return m, func() tea.Msg {
			return MsgScopeReloadRequested{IncludeIgnored: m.scopeOptions.IncludeIgnored, Status: status}
		}

	case MsgSyncError:
		if msg.Conn != nil && msg.Conn != m.conn {
			return m, nil
		}
		m.quickSyncing = false
		m.finishActivity()
		if progress.IsCanceled(msg.Err) && m.disconnected == nil {
			return m, nil
		}
		log.Error("sync error", "err", msg.Err)
		if msg.SessionIdx >= 0 && msg.SessionIdx < len(m.sessions) {
			m.sessions[msg.SessionIdx].Err = msg.Err
			delete(m.completed, msg.SessionIdx)
		}
	}
	return m, nil
}

// startBulkSync initializes the live progress tracker and kicks off the bulk
// sync alongside the periodic progress tick.
func (m Model) startBulkSync(indices []int) (Model, tea.Cmd) {
	if m.remoteBusy() || m.connectionError() != nil {
		return m, nil
	}
	m.syncing = true
	m.syncStatus = ""
	m.syncDone = 0
	m.syncTotal = len(indices)
	m.syncProgress = m.beginActivity("Syncing files…", len(indices))
	return m, tea.Batch(m.bulkSyncCmd(indices), syncProgressTickCmd(m.syncProgress))
}

func (m Model) remoteBusy() bool {
	return m.syncing || m.quickSyncing
}

func (m Model) handleKey(msg tea.KeyMsg) (next Model, cmd tea.Cmd) {
	anchor, flip, active := m.topSourceLine(), m.remoteBeforeLocal(), m.activeIdx
	defer func() {
		next.prepareDisplayRows()
		if next.activeIdx == active && next.remoteBeforeLocal() != flip {
			next.remapScrollToSource(anchor)
		}
	}()
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "q" || msg.String() == "esc" {
			m.showHelp = false
		}
		return m, nil
	}
	if m.showErrors {
		switch msg.String() {
		case "e", "q", "esc":
			m.showErrors = false
		case "j", "down":
			m.errorOffset++
		case "k", "up":
			m.errorOffset--
		case "pgdown", "ctrl+d":
			m.errorOffset += max(1, m.viewportHeight()-1)
		case "pgup", "ctrl+u":
			m.errorOffset -= max(1, m.viewportHeight()-1)
		case "g", "home":
			m.errorOffset = 0
		case "G", "end":
			m.errorOffset = len(m.errorRows(m.diffWidth()))
		}
		m.clampErrorScroll()
		return m, nil
	}
	if m.conn != nil {
		m.ConnectionLost(m.conn, m.conn.Err())
	}
	switch msg.String() {
	case "j", "down", "J", "k", "up", "K", "ctrl+d", "ctrl+u", "pgdown", "pgup", "home", "g", "end", "G":
		m.hunk = nil
		if before := m.scroll; m.continuous() {
			// Only real movement re-targets sync keys; a clamped key press
			// must not take over the file selected with Tab or a hunk jump.
			defer func() {
				if next.scroll != before {
					next.followScroll()
				}
			}()
		}
	}
	switch msg.String() {
	case "?":
		m.showHelp = true
	case "m":
		m.setContinuous(!m.continuous())
	case "z":
		if m.continuous() {
			m.toggleSection(m.activeIdx)
		}
	case "f":
		m.showPaths = !m.showPaths
		m.clampScroll()
	case "v":
		switch m.preferences.Layout {
		case "", "unified":
			m.preferences.Layout = "split"
		case "split":
			m.preferences.Layout = "auto"
		default:
			m.preferences.Layout = "unified"
		}
		m.prepareDisplayRows()
		m.remapScrollToSource(anchor)
		m.clampHorizontal()
	case "<":
		m.resizeFileList(-fileListStep)
	case ">":
		m.resizeFileList(fileListStep)
	case "left":
		m.horizontal -= 8
		m.clampHorizontal()
	case "right":
		if _, ready := m.columnLimit(); ready {
			m.horizontal += 8
			m.clampHorizontal()
		}

	// ── File list navigation ───────────────────────────────────────────
	case "tab", "n":
		if m.activeIdx < len(m.sessions)-1 {
			m.selectFile(m.activeIdx + 1)
		}

	case "shift+tab", "p":
		if m.activeIdx > 0 {
			m.selectFile(m.activeIdx - 1)
		}

	// ── Sync direction — current file (Space) or all files (A) ───────────
	case " ":
		if m.remoteBusy() || m.connectionError() != nil {
			return m, nil
		}
		if m.activeIdx >= 0 && m.activeIdx < len(m.sessions) {
			m.syncDirs[m.activeIdx] = nextDir(m.syncDirs[m.activeIdx], &m.sessions[m.activeIdx])
		}

	case "A":
		if m.remoteBusy() || m.connectionError() != nil {
			return m, nil
		}
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
		m.jumpHunk(1)
	case "[":
		m.jumpHunk(-1)

	case "enter", "l":
		if m.continuous() {
			m.toggleFirstVisibleStreamFold()
		} else {
			m.toggleFirstVisibleFold()
		}
	case "h":
		if m.continuous() {
			m.collapseVisibleStreamFold()
		} else {
			m.collapseVisibleFold()
		}
	case "c":
		m.toggleAllFolds()
	case "L":
		m.revealFirstVisibleFold()
	case "o":
		m.toggleLong(m.activeIdx)
	case "+", "=":
		m.shiftContext(1)
	case "-":
		m.shiftContext(-1)

	// ── Sync: current file with planned direction ──────────────────────
	case "s":
		if !m.remoteBusy() && m.connectionError() == nil && m.activeIdx >= 0 && m.activeIdx < len(m.syncDirs) {
			if m.syncDirs[m.activeIdx] != DirNone {
				return m.startBulkSync([]int{m.activeIdx})
			}
		}

	// ── Sync: all files with planned directions ────────────────────────
	case "S":
		if !m.remoteBusy() && m.connectionError() == nil {
			indices := make([]int, len(m.sessions))
			for i := range indices {
				indices[i] = i
			}
			return m.startBulkSync(indices)
		}

	// ── Quick upload/download (bypass planned direction) ───────────────
	case "u":
		if s := m.activeSession(); !m.remoteBusy() && m.connectionError() == nil && s != nil && s.Result != nil && !s.Result.RemoteOnly {
			m.quickSyncing = true
			m.beginActivity("Uploading file…", 0)
			return m, m.uploadCmd(m.activeIdx)
		}

	case "d":
		if s := m.activeSession(); !m.remoteBusy() && m.connectionError() == nil && s != nil && s.Result != nil && !s.Result.LocalOnly {
			m.quickSyncing = true
			m.beginActivity("Downloading file…", 0)
			return m, m.downloadCmd(m.activeIdx)
		}

	// ── Rebuild recursive scope with/without gitignored paths ──────────
	case "i":
		if !m.remoteBusy() {
			return m, func() tea.Msg {
				return MsgScopeReloadRequested{IncludeIgnored: !m.scopeOptions.IncludeIgnored}
			}
		}

	// ── Refresh: rebuild the comparison with the same scope ────────────
	case "r":
		if !m.remoteBusy() && m.connectionError() == nil {
			return m, func() tea.Msg {
				return MsgScopeReloadRequested{IncludeIgnored: m.scopeOptions.IncludeIgnored}
			}
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

// A reached hunk can lie below the clamped viewport top. Remember its logical
// position so repeated jumps advance even when the whole file fits on screen.
type hunkPosition struct {
	plan        *displayCache
	stream      *streamPlan
	row, scroll int
}

func (m *Model) jumpHunk(direction int) {
	if m.continuous() {
		m.jumpStreamHunk(direction)
		return
	}
	m.prepareDisplayRows()
	for index := m.activeIdx; index >= 0 && index < len(m.sessions); index += direction {
		session := &m.sessions[index]
		if session.Err != nil || session.Result == nil || session.Result.Binary || m.longHidden(index) {
			continue
		}
		added, removed := session.Result.Counts()
		if added == 0 && removed == 0 {
			continue
		}
		changedFile := index != m.activeIdx
		if changedFile {
			m.selectFile(index)
		}
		rows := m.displayRows()
		from := min(m.scroll, len(rows))
		if changedFile {
			from = -1
			if direction < 0 {
				from = len(rows)
			}
		} else if m.hunk != nil && m.hunk.plan == m.display && m.hunk.scroll == m.scroll {
			from = m.hunk.row
		}
		for row := from + direction; row >= 0 && row < len(rows); row += direction {
			if rows[row].Kind == diff.DisplayHunkHeader {
				m.scroll = row
				m.clampScroll()
				m.hunk = &hunkPosition{plan: m.display, row: row, scroll: m.scroll}
				return
			}
		}
	}
}
