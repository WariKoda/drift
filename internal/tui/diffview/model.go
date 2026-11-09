// Package diffview implements the file-list + unified-diff screen.
package diffview

import (
	"context"
	"errors"
	"fmt"
	stdsync "sync"
	"time"

	"github.com/WariKoda/drift/internal/app"
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgDiffLoaded is sent when all sessions have been computed.
// Conn is kept open for subsequent sync operations — caller must close it.
// RequestID and Host identify the LoadCmd call this result belongs to: results
// can arrive after the app has moved on (another project was opened), and the
// caller must be able to tell those apart from the result it is waiting for.
type MsgDiffLoaded struct {
	RequestID uint64
	Host      config.Host
	Sessions  []diff.Session
	Conn      remote.Client
	Root      *fs.Root // project root for local reads and writes; caller must close it
	Scope     syncpolicy.ScopeSummary
	Options   syncpolicy.ScopeOptions
}

// MsgDiffError is sent when SSH/SFTP connection or diff loading fails.
// RequestID and Host identify the failed request, see MsgDiffLoaded.
type MsgDiffError struct {
	RequestID uint64
	Host      config.Host
	Err       error
}

// MsgScopeReloadRequested asks the root model to rebuild the comparison with
// different recursive ignore handling.
type MsgScopeReloadRequested struct {
	IncludeIgnored bool
	Status         string
	Errors         []syncpolicy.Failure
	DeletedLocal   []string
	DeletedRemote  []string
}

// MsgBulkSyncDone is sent when bulk sync has finished.
type MsgBulkSyncDone struct {
	DeletedLocal  []string
	DeletedRemote []string
	Conn          remote.Client
	Done          int                  // number of successfully synced files
	Completed     []int                // session indices with confirmed successful operations
	Errors        []syncpolicy.Failure // one entry per failed file
	Err           error                // terminal connection failure; remaining jobs were not started
}

// MsgSyncProgress is emitted periodically while a bulk sync is running.
type MsgSyncProgress struct {
	Done     int
	Total    int
	Finished bool
}

// syncProgressTickCmd periodically polls the sync tracker for UI updates and
// re-arms itself until the tracker reports completion.
func syncProgressTickCmd(tracker *progress.Tracker) tea.Cmd {
	return tea.Tick(100*time.Millisecond, func(time.Time) tea.Msg {
		progress, done := tracker.Snapshot()
		return MsgSyncProgress{Done: progress.Done, Total: progress.Total, Finished: done}
	})
}

// MsgSynced is sent after a successful upload or download.
type MsgSynced struct {
	Conn       remote.Client
	SessionIdx int
	Direction  SyncDir
}

// MsgSyncError is sent when a sync operation fails.
type MsgSyncError struct {
	Conn       remote.Client
	SessionIdx int
	Err        error
}

// SyncDir indicates the planned sync direction for a file.
type SyncDir int

const (
	DirNone         SyncDir = iota // — no sync planned (zero value / default)
	DirUpload                      // ↑ push local → remote
	DirDownload                    // ↓ pull remote → local
	DirDeleteLocal                 // ✗ delete the local file
	DirDeleteRemote                // ✗ delete the remote file
)

func syncDirFromDecision(decision syncpolicy.Decision) SyncDir {
	switch decision {
	case syncpolicy.DecisionUpload:
		return DirUpload
	case syncpolicy.DecisionDownload:
		return DirDownload
	case syncpolicy.DecisionDeleteLocal:
		return DirDeleteLocal
	case syncpolicy.DecisionDeleteRemote:
		return DirDeleteRemote
	default:
		return DirNone
	}
}

func decisionFromSyncDir(dir SyncDir) syncpolicy.Decision {
	switch dir {
	case DirUpload:
		return syncpolicy.DecisionUpload
	case DirDownload:
		return syncpolicy.DecisionDownload
	case DirDeleteLocal:
		return syncpolicy.DecisionDeleteLocal
	case DirDeleteRemote:
		return syncpolicy.DecisionDeleteRemote
	default:
		return syncpolicy.DecisionNone
	}
}

func autoDir(s *diff.Session) SyncDir {
	return syncDirFromDecision(syncpolicy.AutoDecision(s))
}

func nextDir(cur SyncDir, s *diff.Session) SyncDir {
	return syncDirFromDecision(syncpolicy.NextDecision(decisionFromSyncDir(cur), s))
}

// commandLifetime keeps the local root alive for running commands. Queued
// commands may never run during Bubble Tea shutdown, so Close must reject them
// rather than waiting for a completion that the runtime cannot guarantee.
type commandLifetime struct {
	mu      stdsync.Mutex
	closed  bool
	running stdsync.WaitGroup
}

// Model is the diff view screen.
type Model struct {
	sessions        []diff.Session
	syncDirs        []SyncDir // planned direction per session (index-aligned)
	activeIdx       int
	fileListOffset  int // first visible sidebar row, headings included
	fileListColumns int // sidebar width chosen with [<]/[>]; 0 is automatic
	listing         *fileListing
	scroll          int
	syncing         bool                 // true while bulk sync is in flight
	quickSyncing    bool                 // true while quick upload/download is in flight
	activityLabel   string               // label shown by the global loading indicator
	activityTracker *progress.Tracker    // progress shared with the global loading indicator
	activityWait    *commandLifetime     // protects the root handle until running commands exit
	syncStatus      string               // last bulk sync result message
	syncErrors      []syncpolicy.Failure // per-file errors from the last bulk sync
	showErrors      bool                 // true while the error overlay is open
	errorOffset     int
	syncProgress    *progress.Tracker // live counter shared with the running bulk sync
	syncDone        int               // files processed so far in the active bulk sync
	syncTotal       int               // total files in the active bulk sync
	scope           syncpolicy.ScopeSummary
	scopeOptions    syncpolicy.ScopeOptions
	host            config.Host
	conn            remote.Client // kept open for sync ops, including after connection loss
	disconnected    error         // sticky until a new model/comparison is opened
	closed          bool
	completed       map[int]bool // confirmed sync successes, retained when refresh cannot run
	root            *fs.Root     // confines every local read, write and delete to the project
	clicks          mouse.ClickTracker
	expandedGaps    map[int]map[int]struct{} // session index → expanded GapIDs
	revealed        map[int]map[int]int      // session index → GapID → extra context lines
	contextShift    int                      // [+]/[-] steps away from diff.DefaultContext
	display         *displayCache
	stream          *streamPlan  // all-files layout in continuous mode
	collapsed       map[int]bool // sessions reduced to their title in continuous mode
	longOpen        map[int]bool // long-line files whose diff was opened with [o]
	hunk            *hunkPosition
	foldRevision    uint64
	savedScroll     map[int]int
	savedSource     map[int]int
	decoration      *decorationRequest
	decorated       map[*diff.DiffResult]*fileDecoration
	syntaxCache     *diff.SyntaxCache
	preferences     config.DiffPreferences
	horizontal      int
	savedHorizontal map[int]int
	showHelp        bool
	showPaths       bool // full local and remote paths below the file title
	Width           int
	Height          int
}

// New creates a Model with pre-loaded sessions.
// syncDirs are pre-filled by autoDir so the user starts with a sensible selection.
func New(sessions []diff.Session, host config.Host, conn remote.Client, root *fs.Root, width, height int) Model {
	syncDirs := make([]SyncDir, len(sessions))
	for i := range sessions {
		syncDirs[i] = autoDir(&sessions[i])
	}
	model := Model{
		sessions:     sessions,
		syncDirs:     syncDirs,
		host:         host,
		conn:         conn,
		root:         root,
		Width:        width,
		Height:       height,
		expandedGaps: map[int]map[int]struct{}{},
		activityWait: &commandLifetime{},
	}
	model.listing = model.buildFileList()
	model.prepareDisplayRows()
	model.scrollToFirstDifference()
	return model
}

// SetScope attaches the immutable scope summary used to build these sessions.
func (m *Model) SetScope(scope syncpolicy.ScopeSummary, options syncpolicy.ScopeOptions) {
	m.scope = scope
	m.scopeOptions = options
}

// SetSyncResult restores the result of a transfer across a full scope reload.
func (m *Model) SetSyncResult(status string, failures []syncpolicy.Failure) {
	m.syncStatus = status
	m.syncErrors = append([]syncpolicy.Failure(nil), failures...)
	m.showErrors = len(failures) > 0
	m.errorOffset = 0
}

// Init satisfies the sub-model convention.
func (m *Model) Init() tea.Cmd { return m.startDecoration() }

// LoadingActivity exposes the current network activity to the root indicator.
func (m Model) LoadingActivity() (string, *progress.Tracker, bool) {
	return m.activityLabel, m.activityTracker, m.remoteBusy()
}

// CancelActivity asks an in-flight sync, refresh, or upload to stop. Work
// already written is kept; remaining files are left alone.
func (m *Model) CancelActivity() {
	if m.activityTracker != nil {
		m.activityTracker.Cancel()
	}
}

func (m *Model) beginActivity(label string, total int) *progress.Tracker {
	if m.activityWait == nil {
		m.activityWait = &commandLifetime{}
	}
	tracker := progress.NewTracker(label)
	if total > 0 {
		tracker.Set(label, 0, total, false)
	}
	m.activityLabel = label
	m.activityTracker = tracker
	return tracker
}

func (m *Model) finishActivity() {
	m.activityLabel = ""
	m.activityTracker = nil
}

// trackCommand admits work under the same lock Close uses to stop admission.
// A command dequeued after Close cannot touch the already released root.
func (m Model) trackCommand(cmd tea.Cmd) tea.Cmd {
	wait := m.activityWait
	tracker := m.activityTracker
	conn := m.conn
	if wait == nil {
		return cmd
	}
	return func() tea.Msg {
		wait.mu.Lock()
		if wait.closed {
			wait.mu.Unlock()
			return nil
		}
		wait.running.Add(1)
		wait.mu.Unlock()
		defer wait.running.Done()
		if tracker != nil && conn != nil {
			detach := remote.CloseOnContextDone(tracker.Context(), conn)
			defer detach()
		}
		return cmd()
	}
}

// Connection returns the owned connection, even after failure, until Close detaches it.
func (m Model) Connection() remote.Client { return m.conn }

// ConnectionLost applies only to the current connection. Commands retain their
// captured handles and report their outcomes; no new work is allowed afterwards.
func (m *Model) ConnectionLost(conn remote.Client, err error) bool {
	if conn == nil || conn != m.conn || err == nil || m.closed || m.disconnected != nil {
		return false
	}
	m.disconnected = err
	m.CancelActivity()
	return true
}

// connectionError also checks the client directly: its monitor can fail before
// the root's connection-status message reaches this screen.
func (m Model) connectionError() error {
	if m.disconnected != nil {
		return m.disconnected
	}
	if m.closed || m.conn == nil {
		return errors.New("connection is closed")
	}
	if err := m.conn.Err(); err != nil {
		return err
	}
	select {
	case <-m.conn.Done():
		return errors.New("connection is closed")
	default:
		return nil
	}
}

// Close detaches ownership and cancels pending work immediately. Network close
// may wait for an active operation, so resource cleanup belongs in the command.
func (m *Model) Close() tea.Cmd {
	if m.decoration != nil {
		m.decoration.cancel()
	}
	m.CancelActivity()
	conn, root, host := m.conn, m.root, m.host.Name
	wait := m.activityWait
	if wait != nil {
		wait.mu.Lock()
		wait.closed = true
		wait.mu.Unlock()
	}
	m.conn, m.root = nil, nil
	m.closed = true
	m.syncing, m.quickSyncing = false, false
	m.syncProgress = nil
	m.finishActivity()
	if conn == nil && root == nil {
		return nil
	}
	return func() tea.Msg {
		if conn != nil {
			log.Info("remote disconnect", "host", host)
			if err := conn.Close(); err != nil {
				log.Error("remote disconnect failed", "host", host, "err", err)
			}
		}
		if wait != nil {
			wait.running.Wait()
		}
		if root != nil {
			if err := root.Close(); err != nil {
				log.Error("close project root failed", "err", err)
			}
		}
		return nil
	}
}

// SetSize updates terminal dimensions and keeps the diff viewport in bounds.
func (m *Model) SetSize(w, h int) {
	anchor, split := m.topSourceLine(), m.splitLayout()
	m.Width = max(0, w)
	m.Height = max(0, h)
	m.prepareDisplayRows()
	m.clampFileList()
	m.clampErrorScroll()
	if split != m.splitLayout() {
		m.remapScrollToSource(anchor)
	}
	m.clampScroll()
	m.clampHorizontal()
}

// bodyHeight returns the shared vertical space for the file list and diff pane.
// See the layout constants in view.go for the row budget.
func (m Model) bodyHeight() int {
	h := m.Height - headerLines - footerLines
	if h < 1 {
		return 1
	}
	return h
}

// fileListHeight returns the number of visible rows in the left file list.
// Side-by-side layout uses the full body height.
func (m Model) fileListHeight() int {
	return m.bodyHeight()
}

// viewportHeight returns available lines for unified diff content in the right
// pane (body minus the path header chrome).
func (m Model) viewportHeight() int {
	h := m.bodyHeight() - m.chromeRows()
	if h < 1 {
		return 1
	}
	return h
}

// clampFileList keeps activeIdx in bounds and the file list scrolled to show
// it, together with its directory heading when the file opens a group.
func (m *Model) clampFileList() {
	if len(m.sessions) == 0 {
		return
	}
	if m.activeIdx < 0 {
		m.activeIdx = 0
	}
	if m.activeIdx >= len(m.sessions) {
		m.activeIdx = len(m.sessions) - 1
	}
	list := m.fileList()
	row := list.rowOf[m.activeIdx]
	top := row
	if top > 0 && list.rows[top-1].session < 0 {
		top--
	}
	fh := m.fileListHeight()
	if top < m.fileListOffset {
		m.fileListOffset = top
	}
	if row >= m.fileListOffset+fh {
		m.fileListOffset = row - fh + 1
	}
	m.clampScroll()
}

// clampFileListOffset keeps the file list offset in range without dragging it
// back to activeIdx. The wheel moves the viewport on its own, so the regular
// clampFileList — which exists to follow the cursor — must not run after it.
func (m *Model) clampFileListOffset() {
	max := len(m.fileList().rows) - m.fileListHeight()
	if max < 0 {
		max = 0
	}
	if m.fileListOffset > max {
		m.fileListOffset = max
	}
	if m.fileListOffset < 0 {
		m.fileListOffset = 0
	}
}

// fileListWidth returns the left sidebar width for the file list.
func (m Model) fileListWidth() int {
	if m.Width < 40 {
		return 0
	}
	w := min(max(m.Width/3, 22), 40)
	if m.fileListColumns > 0 {
		w = m.fileListColumns
	}
	// Leave enough room for the diff gutter and a few content columns.
	return min(max(w, 16), m.Width-1-20)
}

// diffWidth returns the right pane width (path header + unified diff).
func (m Model) diffWidth() int {
	width := m.Width - m.fileListWidth()
	if m.fileListWidth() > 0 {
		width--
	}
	return max(0, width)
}

// activeSession returns the current session or nil.
func (m Model) activeSession() *diff.Session {
	if m.activeIdx < 0 || m.activeIdx >= len(m.sessions) {
		return nil
	}
	return &m.sessions[m.activeIdx]
}

// totalLines returns the number of scrollable rows: the active session's
// display rows, or every file's rows in continuous mode.
func (m Model) totalLines() int {
	if m.continuous() {
		return len(m.streamPlan().rows)
	}
	return len(m.displayRows())
}

func (m Model) displayRows() []diff.DisplayRow {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return nil
	}
	if m.display != nil && m.display.result == s.Result && m.display.flip == m.remoteBeforeLocal() && m.display.revision == m.foldRevision && m.display.split == m.splitLayout() {
		return m.display.rows
	}
	return m.rowsFor(m.activeIdx)
}

type displayCache struct {
	split    bool
	result   *diff.DiffResult
	flip     bool
	revision uint64
	rows     []diff.DisplayRow
}

func (m *Model) prepareDisplayRows() {
	defer m.prepareStream()
	s := m.activeSession()
	if s == nil || s.Result == nil {
		m.display = nil
		if !m.continuous() {
			m.hunk = nil
		}
		return
	}
	if m.display != nil && m.display.result == s.Result && m.display.flip == m.remoteBeforeLocal() && m.display.revision == m.foldRevision && m.display.split == m.splitLayout() {
		return
	}
	if !m.continuous() {
		m.hunk = nil
	}
	m.display = &displayCache{result: s.Result, flip: m.remoteBeforeLocal(), revision: m.foldRevision, split: m.splitLayout(), rows: m.displayRows()}
}

func (m Model) remoteBeforeLocal() bool {
	return m.sessionFlip(m.activeIdx)
}

func (m Model) expandedFor(sessionIdx int) map[int]struct{} {
	if m.expandedGaps == nil {
		return nil
	}
	return m.expandedGaps[sessionIdx]
}

func (m *Model) resetFolds(sessionIdx int) {
	m.foldRevision++
	delete(m.expandedGaps, sessionIdx)
	delete(m.revealed, sessionIdx)
}

func (m *Model) ensureExpanded(sessionIdx int) map[int]struct{} {
	if m.expandedGaps == nil {
		m.expandedGaps = map[int]map[int]struct{}{}
	}
	set := m.expandedGaps[sessionIdx]
	if set == nil {
		set = map[int]struct{}{}
		m.expandedGaps[sessionIdx] = set
	}
	return set
}

func (m *Model) gapExpanded(gapID int) bool { return m.gapExpandedIn(m.activeIdx, gapID) }

func (m *Model) gapExpandedIn(session, gapID int) bool {
	_, ok := m.expandedFor(session)[gapID]
	return ok
}

func (m *Model) setGapExpanded(gapID int, open bool) { m.setGapExpandedIn(m.activeIdx, gapID, open) }

func (m *Model) setGapExpandedIn(session, gapID int, open bool) {
	m.foldRevision++
	if !open {
		if set := m.expandedFor(session); set != nil {
			delete(set, gapID)
		}
		return
	}
	m.ensureExpanded(session)[gapID] = struct{}{}
}

// remapScrollToSource places a source line of the active file at the top. In
// continuous mode the rebuilt plan keeps its own anchor instead.
func (m *Model) remapScrollToSource(lineIdx int) {
	m.prepareDisplayRows()
	if !m.continuous() {
		m.scroll = diff.IndexOfSourceLine(m.displayRows(), lineIdx)
	}
	m.clampScroll()
}

func (m *Model) toggleGap(gapID int) {
	rows := m.displayRows()
	anchor := 0
	if m.scroll >= 0 && m.scroll < len(rows) {
		anchor = rows[m.scroll].SourceLine()
	}
	m.setGapExpanded(gapID, !m.gapExpanded(gapID))
	m.remapScrollToSource(anchor)
}

func (m *Model) firstVisibleFold() (diff.DisplayRow, bool) {
	rows := m.displayRows()
	end := m.scroll + m.rowsOnScreen()
	if end > len(rows) {
		end = len(rows)
	}
	for i := m.scroll; i < end; i++ {
		if rows[i].Kind == diff.DisplayFold {
			return rows[i], true
		}
	}
	return diff.DisplayRow{}, false
}

func (m *Model) toggleFirstVisibleFold() {
	row, ok := m.firstVisibleFold()
	if !ok {
		return
	}
	m.toggleGap(row.GapID)
}

func (m *Model) collapseVisibleFold() {
	rows := m.displayRows()
	if len(rows) == 0 {
		return
	}
	top := min(max(0, m.scroll), len(rows)-1)
	bottom := min(m.scroll+m.rowsOnScreen()-1, len(rows)-1)
	anchor := rows[top].SourceLine()
	if m.collapseFoldIn(m.activeIdx, rows, top, bottom) {
		m.remapScrollToSource(anchor)
	}
}

// collapseFoldIn closes the first expanded gap of a session that overlaps its
// display rows top…bottom.
func (m *Model) collapseFoldIn(session int, rows []diff.DisplayRow, top, bottom int) bool {
	s := &m.sessions[session]
	if s.Result == nil {
		return false
	}
	// A hunk header names its change run but precedes the leading context, so
	// the visible source range is the span over all rows, not the first row.
	viewStart, viewEnd := len(s.Result.Lines), -1
	for _, row := range rows[top : bottom+1] {
		first, last := row.SourceLine(), row.SourceLine()
		if row.Kind == diff.DisplayFold {
			last = row.FoldEnd - 1
		}
		viewStart, viewEnd = min(viewStart, first), max(viewEnd, last)
	}
	for _, id := range diff.FoldableGapIDs(s.Result.Lines, m.context()) {
		if !m.gapExpandedIn(session, id) && m.revealed[session][id] == 0 {
			continue
		}
		end := equalRunEnd(s.Result.Lines, id)
		if end > viewStart && id <= viewEnd {
			delete(m.revealed[session], id)
			m.setGapExpandedIn(session, id, false)
			return true
		}
	}
	return false
}

func equalRunEnd(lines []diff.DiffLine, start int) int {
	i := start
	for i < len(lines) && lines[i].Kind == diff.LineEqual {
		i++
	}
	return i
}

func (m *Model) toggleAllFolds() {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return
	}
	ids := diff.FoldableGapIDs(s.Result.Lines, m.context())
	if len(ids) == 0 {
		return
	}
	rows := m.displayRows()
	anchor := 0
	if m.scroll >= 0 && m.scroll < len(rows) {
		anchor = rows[m.scroll].SourceLine()
	}
	anyOpen := len(m.revealed[m.activeIdx]) > 0
	for _, id := range ids {
		if m.gapExpanded(id) {
			anyOpen = true
			break
		}
	}
	if anyOpen {
		m.resetFolds(m.activeIdx)
	} else {
		set := make(map[int]struct{}, len(ids))
		for _, id := range ids {
			set[id] = struct{}{}
		}
		if m.expandedGaps == nil {
			m.expandedGaps = map[int]map[int]struct{}{}
		}
		m.expandedGaps[m.activeIdx] = set
		m.foldRevision++
	}
	m.remapScrollToSource(anchor)
}

func (m *Model) clampScroll() {
	max := m.scrollLimit()
	if m.scroll > max {
		m.scroll = max
	}
	if m.scroll < 0 {
		m.scroll = 0
	}
}

// scrollToFirstDifference places the first hunk header (or changed row) at the
// top of the viewport, or as close to it as the remaining number of rows allows.
func (m *Model) scrollToFirstDifference() {
	m.scroll = 0
	session := m.activeSession()
	if session == nil || session.Err != nil || session.Result == nil || session.Result.Binary {
		return
	}
	for index, row := range m.displayRows() {
		if row.Kind == diff.DisplayHunkHeader {
			m.scroll = index
			m.clampScroll()
			return
		}
	}
}

// LoadCmd runs app.Load asynchronously and reports MsgDiffLoaded or
// MsgDiffError. requestID is echoed back in the result so the caller can
// discard results of requests it has abandoned in the meantime.
func LoadCmd(requestID uint64, req app.LoadRequest, prog *progress.Tracker) tea.Cmd {
	return func() tea.Msg {
		defer prog.Finish()
		result, err := app.Load(prog.Context(), req, prog)
		if err != nil {
			return MsgDiffError{RequestID: requestID, Host: req.Host, Err: err}
		}
		return MsgDiffLoaded{
			RequestID: requestID,
			Host:      req.Host,
			Sessions:  result.Sessions,
			Conn:      result.Conn,
			Root:      result.Root,
			Scope:     result.Scope,
			Options:   req.Options,
		}
	}
}

// quickSyncCmd runs one upload or download for sessions[idx].
func (m Model) quickSyncCmd(idx int, dir SyncDir) tea.Cmd {
	s := m.sessions[idx]
	conn := m.conn
	root := m.root
	tracker := m.activityTracker
	return m.trackCommand(func() tea.Msg {
		defer tracker.Finish()
		if err := m.connectionError(); err != nil {
			return MsgSyncError{Conn: conn, SessionIdx: idx, Err: err}
		}
		if tracker.Canceled() {
			return MsgSyncError{Conn: conn, SessionIdx: idx, Err: context.Canceled}
		}
		item := syncpolicy.Item{LocalPath: s.LocalPath, RemotePath: s.RemotePath, Decision: decisionFromSyncDir(dir)}
		result := syncpolicy.Run(tracker.Context(), conn, root, []syncpolicy.Item{item}, tracker)
		if len(result.Completed) == 1 {
			return MsgSynced{Conn: conn, SessionIdx: idx, Direction: dir}
		}
		if len(result.Failures) == 0 {
			if result.Err == nil {
				result.Err = context.Canceled
			}
			return MsgSyncError{Conn: conn, SessionIdx: idx, Err: result.Err}
		}
		path := s.LocalPath
		if dir == DirDownload {
			path = s.RemotePath
		}
		return MsgSyncError{Conn: conn, SessionIdx: idx, Err: fmt.Errorf("%s %s: %w", result.Failures[0].Operation, path, result.Failures[0].Err)}
	})
}

// uploadCmd uploads the local file of sessions[idx] to remote.
func (m Model) uploadCmd(idx int) tea.Cmd { return m.quickSyncCmd(idx, DirUpload) }

// downloadCmd downloads the remote file of sessions[idx] to local.
func (m Model) downloadCmd(idx int) tea.Cmd { return m.quickSyncCmd(idx, DirDownload) }

// bulkSyncCmd executes the planned sync direction for the given session indices.
func (m Model) bulkSyncCmd(indices []int) tea.Cmd {
	sessions := append([]diff.Session(nil), m.sessions...)
	syncDirs := append([]SyncDir(nil), m.syncDirs...)
	conn := m.conn
	root := m.root
	tracker := m.syncProgress
	return m.trackCommand(func() tea.Msg {
		defer tracker.Finish()
		if err := m.connectionError(); err != nil {
			return MsgBulkSyncDone{Conn: conn, Err: err}
		}
		var items []syncpolicy.Item
		var sessionIdx []int // session index of each item
		for _, i := range indices {
			if i < 0 || i >= len(sessions) || i >= len(syncDirs) {
				tracker.Inc()
				continue
			}
			s := sessions[i]
			items = append(items, syncpolicy.Item{LocalPath: s.LocalPath, RemotePath: s.RemotePath, Decision: decisionFromSyncDir(syncDirs[i])})
			sessionIdx = append(sessionIdx, i)
		}
		result := syncpolicy.Run(tracker.Context(), conn, root, items, tracker)
		out := MsgBulkSyncDone{Conn: conn, Done: len(result.Completed), Errors: result.Failures, Err: result.Err}
		for _, index := range result.Completed {
			out.Completed = append(out.Completed, sessionIdx[index])
			switch items[index].Decision {
			case syncpolicy.DecisionDeleteLocal:
				out.DeletedLocal = append(out.DeletedLocal, items[index].LocalPath)
			case syncpolicy.DecisionDeleteRemote:
				out.DeletedRemote = append(out.DeletedRemote, items[index].RemotePath)
			}
		}
		return out
	})
}
