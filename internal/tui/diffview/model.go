// Package diffview implements the file-list + unified-diff screen.
package diffview

import (
	"context"
	"errors"
	"fmt"
	"net/textproto"
	"os"
	"sort"
	"strings"
	stdsync "sync"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/pathmap"
	"github.com/WariKoda/drift/internal/remote"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	"github.com/WariKoda/drift/internal/tui/loading"
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
}

// LoadProgressTracker shares operation progress with the global indicator.
type LoadProgressTracker = loading.Tracker

// NewLoadProgressTracker creates a tracker initialized to the first loading phase.
func NewLoadProgressTracker() *LoadProgressTracker {
	return loading.NewTracker("Connecting…")
}

// MsgDiffError is sent when SSH/SFTP connection or diff loading fails.
// RequestID and Host identify the failed request, see MsgDiffLoaded.
type MsgDiffError struct {
	RequestID uint64
	Host      config.Host
	Err       error
}

// MsgRefreshed is sent when a full diff refresh has completed.
type MsgRefreshed struct{ Sessions []diff.Session }

// SyncFailure describes one failed operation in a bulk sync.
type SyncFailure struct {
	Operation string
	Path      string
	Reason    string
}

// MsgBulkSyncDone is sent when bulk sync has finished.
type MsgBulkSyncDone struct {
	Done   int           // number of successfully synced files
	Errors []SyncFailure // one entry per failed file
}

// MsgSyncProgress is emitted periodically while a bulk sync is running.
type MsgSyncProgress struct {
	Done     int
	Total    int
	Finished bool
}

// syncProgressTickCmd periodically polls the sync tracker for UI updates and
// re-arms itself until the tracker reports completion.
func syncProgressTickCmd(tracker *LoadProgressTracker) tea.Cmd {
	return tea.Tick(100*time.Millisecond, func(time.Time) tea.Msg {
		progress, done := tracker.Snapshot()
		return MsgSyncProgress{Done: progress.Done, Total: progress.Total, Finished: done}
	})
}

// MsgSynced is sent after a successful upload or download.
type MsgSynced struct {
	SessionIdx int
	Direction  SyncDir
}

// MsgSyncError is sent when a sync operation fails.
type MsgSyncError struct{ Err error }

// MsgSessionReloaded is sent after a quick sync re-compares the changed file.
type MsgSessionReloaded struct {
	SessionIdx int
	Result     *diff.DiffResult
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

// Model is the diff view screen.
type Model struct {
	sessions        []diff.Session
	syncDirs        []SyncDir // planned direction per session (index-aligned)
	activeIdx       int
	fileListOffset  int // scroll offset into the file list
	scroll          int
	refreshing      bool                 // true while async refresh is in flight
	syncing         bool                 // true while bulk sync is in flight
	quickSyncing    bool                 // true while quick upload/download is in flight
	activityLabel   string               // label shown by the global loading indicator
	activityTracker *LoadProgressTracker // progress shared with the global loading indicator
	syncStatus      string               // last bulk sync result message
	syncErrors      []SyncFailure        // per-file errors from the last bulk sync
	showErrors      bool                 // true while the error overlay is open
	syncProgress    *LoadProgressTracker // live counter shared with the running bulk sync
	syncDone        int                  // files processed so far in the active bulk sync
	syncTotal       int                  // total files in the active bulk sync
	host            config.Host
	conn            remote.Client // kept open for sync ops
	root            *fs.Root      // confines every local read, write and delete to the project
	clicks          mouse.ClickTracker
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
		sessions: sessions,
		syncDirs: syncDirs,
		host:     host,
		conn:     conn,
		root:     root,
		Width:    width,
		Height:   height,
	}
	model.scrollToFirstDifference()
	return model
}

// Init satisfies the sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// LoadingActivity exposes the current network activity to the root indicator.
func (m Model) LoadingActivity() (string, *LoadProgressTracker, bool) {
	return m.activityLabel, m.activityTracker, m.remoteBusy()
}

func (m *Model) beginActivity(label string, total int) *LoadProgressTracker {
	tracker := loading.NewTracker(label)
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

// Close releases the remote connection and the project root handle. Call when
// leaving the diff view.
func (m *Model) Close() {
	if m.conn != nil {
		log.Info("remote disconnect", "host", m.host.Name)
		_ = m.conn.Close()
		m.conn = nil
	}
	if m.root != nil {
		_ = m.root.Close()
		m.root = nil
	}
}

// SetSize updates terminal dimensions and keeps the diff viewport in bounds.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
	m.clampFileList()
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
	h := m.bodyHeight() - pathChrome
	if h < 1 {
		return 1
	}
	return h
}

// clampFileList keeps activeIdx in bounds and the file list scrolled to show it.
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
	fh := m.fileListHeight()
	if m.activeIdx < m.fileListOffset {
		m.fileListOffset = m.activeIdx
	}
	if m.activeIdx >= m.fileListOffset+fh {
		m.fileListOffset = m.activeIdx - fh + 1
	}
	m.clampScroll()
}

// clampFileListOffset keeps the file list offset in range without dragging it
// back to activeIdx. The wheel moves the viewport on its own, so the regular
// clampFileList — which exists to follow the cursor — must not run after it.
func (m *Model) clampFileListOffset() {
	max := len(m.sessions) - m.fileListHeight()
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
	w := m.Width / 3
	if w < 22 {
		w = 22
	}
	if w > 40 {
		w = 40
	}
	// Leave enough room for the diff gutter and a few content columns.
	max := m.Width - 1 - 20
	if max < 10 {
		max = 10
	}
	if w > max {
		w = max
	}
	return w
}

// diffWidth returns the right pane width (path header + unified diff).
func (m Model) diffWidth() int {
	w := m.Width - m.fileListWidth() - 1 // 1 for │ divider
	if w < 10 {
		return 10
	}
	return w
}

// activeSession returns the current session or nil.
func (m Model) activeSession() *diff.Session {
	if m.activeIdx < 0 || m.activeIdx >= len(m.sessions) {
		return nil
	}
	return &m.sessions[m.activeIdx]
}

// totalLines returns the number of diff lines in the active session.
func (m Model) totalLines() int {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return 0
	}
	return len(s.Result.Lines)
}

func (m *Model) clampScroll() {
	max := m.totalLines() - m.viewportHeight()
	if max < 0 {
		max = 0
	}
	if m.scroll > max {
		m.scroll = max
	}
	if m.scroll < 0 {
		m.scroll = 0
	}
}

// scrollToFirstDifference places the first changed text row at the top of the
// viewport, or as close to it as the remaining number of rows allows.
func (m *Model) scrollToFirstDifference() {
	m.scroll = 0
	session := m.activeSession()
	if session == nil || session.Err != nil || session.Result == nil || session.Result.Binary {
		return
	}
	for index, line := range session.Result.Lines {
		if line.Kind != diff.LineEqual {
			m.scroll = index
			m.clampScroll()
			return
		}
	}
}

// LoadCmd returns a tea.Cmd that connects to host and loads all diffs asynchronously.
// Marked directories are expanded recursively. Local selections also walk the
// mapped remote directory to catch remote-only files; remote selections do the
// inverse and walk the mapped local directory to catch local-only files.
// requestID is echoed back in the result so the caller can discard results of
// requests it has abandoned in the meantime.
func LoadCmd(requestID uint64, host config.Host, localSel, remoteSel *fs.SelectionState, cfg *config.MergedConfig, existingConn remote.Client, progress *LoadProgressTracker) tea.Cmd {
	return func() tea.Msg {
		defer progress.Finish()
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()

		root, err := fs.OpenRoot(cfg.ProjectRoot)
		if err != nil {
			log.Error("open project root failed", "root", cfg.ProjectRoot, "err", err)
			if existingConn != nil {
				_ = existingConn.Close()
			}
			return MsgDiffError{RequestID: requestID, Host: host, Err: err}
		}

		conn := existingConn
		if conn == nil {
			progress.Set("Connecting…", 0, 0, true)
			conn, err = remote.Connect(ctx, host)
			if err != nil {
				log.Error("remote connect failed", "hostname", host.Hostname, "err", err)
				_ = root.Close()
				return MsgDiffError{
					RequestID: requestID,
					Host:      host,
					Err:       fmt.Errorf("connect to %s: %w", host.Hostname, err),
				}
			}
			log.Info("remote connect", "host", host.Name, "hostname", host.Hostname)
		}

		progress.Set("Scanning selections…", 0, 0, true)
		mapper := pathmap.New(cfg.ProjectRoot, cfg.Mappings, host)
		var items []diffLoadItem
		seenPairs := map[string]struct{}{}

		addError := func(localPath, remotePath string, err error) {
			items = append(items, diffLoadItem{LocalPath: localPath, RemotePath: remotePath, Err: err})
		}

		// addFile queues one local/remote file pair for comparison. Identical files
		// are skipped after the parallel compare phase.
		addFile := func(localPath, remotePath string) {
			key := localPath + "\x00" + remotePath
			if _, seen := seenPairs[key]; seen {
				return
			}
			seenPairs[key] = struct{}{}
			items = append(items, diffLoadItem{LocalPath: localPath, RemotePath: remotePath, Compare: true})
		}

		for _, localPath := range sortedMarkedPaths(localSel) {
			info, statErr := root.Stat(localPath)
			if statErr != nil {
				addError(localPath, "", statErr)
				continue
			}

			if !info.IsDir() {
				// ── Single local file ─────────────────────────────────
				remotePath, mapErr := mapper.LocalToRemote(localPath)
				if mapErr != nil {
					addError(localPath, "", mapErr)
					continue
				}
				addFile(localPath, remotePath)
				continue
			}

			// ── Directory: walk local side first ─────────────────────
			seenLocal := map[string]struct{}{}
			if walkErr := fs.WalkFiles(localPath, func(p string) error {
				seenLocal[p] = struct{}{}
				remotePath, mapErr := mapper.LocalToRemote(p)
				if mapErr != nil {
					addError(p, "", mapErr)
					return nil
				}
				addFile(p, remotePath)
				return nil
			}); walkErr != nil {
				addError(localPath, "", fmt.Errorf("walk local: %w", walkErr))
			}

			// ── Walk remote side to catch remote-only files ───────────
			remoteDir, mapErr := mapper.LocalToRemote(localPath)
			if mapErr != nil {
				continue
			}
			if walkErr := conn.WalkFiles(remoteDir, func(remotePath string) error {
				localFilePath, revErr := mapper.RemoteToLocal(remotePath)
				if revErr != nil {
					return nil
				}
				if _, seen := seenLocal[localFilePath]; seen {
					return nil // already covered by local walk
				}
				addFile(localFilePath, remotePath)
				return nil
			}); walkErr != nil {
				addError(localPath, "", fmt.Errorf("walk remote: %w", walkErr))
			}
		}

		for _, remotePath := range sortedMarkedPaths(remoteSel) {
			localPath, mapErr := mapper.RemoteToLocal(remotePath)
			if mapErr != nil {
				addError("", remotePath, mapErr)
				continue
			}

			info, statErr := conn.Stat(remotePath)
			if statErr != nil {
				addError(localPath, remotePath, statErr)
				continue
			}

			if !info.IsDir() {
				// ── Single remote file ────────────────────────────────
				addFile(localPath, remotePath)
				continue
			}

			// ── Directory: walk remote side first ───────────────────
			seenRemote := map[string]struct{}{}
			if walkErr := conn.WalkFiles(remotePath, func(p string) error {
				seenRemote[p] = struct{}{}
				localFilePath, revErr := mapper.RemoteToLocal(p)
				if revErr != nil {
					addError("", p, revErr)
					return nil
				}
				addFile(localFilePath, p)
				return nil
			}); walkErr != nil {
				addError(localPath, remotePath, fmt.Errorf("walk remote: %w", walkErr))
			}

			// ── Walk local side to catch local-only files ────────────
			localInfo, localErr := root.Stat(localPath)
			if localErr != nil {
				if !errors.Is(localErr, os.ErrNotExist) {
					addError(localPath, remotePath, localErr)
				}
				continue
			}
			if !localInfo.IsDir() {
				continue
			}
			if walkErr := fs.WalkFiles(localPath, func(p string) error {
				remoteFilePath, revErr := mapper.LocalToRemote(p)
				if revErr != nil {
					addError(p, "", revErr)
					return nil
				}
				if _, seen := seenRemote[remoteFilePath]; seen {
					return nil // already covered by remote walk
				}
				addFile(p, remoteFilePath)
				return nil
			}); walkErr != nil {
				addError(localPath, remotePath, fmt.Errorf("walk local: %w", walkErr))
			}
		}

		return MsgDiffLoaded{
			RequestID: requestID,
			Host:      host,
			Sessions:  loadDiffItems(root, host, conn, items, progress),
			Conn:      conn,
			Root:      root,
		}
	}
}

const (
	// maxDiffLoadWorkers caps concurrency for SFTP, where every worker shares
	// the single SFTP client. pkg/sftp pipelines concurrent requests over one
	// connection, so extra workers hide per-request round-trip latency — the
	// dominant cost when comparing many small files.
	maxDiffLoadWorkers = 8
	// maxFTPDiffLoadWorkers caps concurrency for FTP, where every worker beyond
	// the first opens its own connection. Connection setup is expensive and
	// servers commonly limit concurrent logins, so this stays low.
	maxFTPDiffLoadWorkers = 4
)

// diffLoadWorkers returns the worker count to use for a host's protocol.
func diffLoadWorkers(host config.Host) int {
	if isFTPProtocol(host.Protocol) {
		return maxFTPDiffLoadWorkers
	}
	return maxDiffLoadWorkers
}

type diffLoadItem struct {
	LocalPath  string
	RemotePath string
	Err        error
	Compare    bool
}

// compareFunc receives a job index plus the connection that worker should use.
type compareFunc func(idx int, conn remote.Client)

// forEachCompare runs fn for every index in jobs across a bounded worker pool.
// The first worker always reuses the connection that is already established, so
// a server allowing only one session per user still produces a complete diff.
// SFTP shares that one connection across all workers because pkg/sftp
// pipelines concurrent requests; for FTP every additional worker needs its own
// connection and is therefore best-effort — a worker that cannot connect simply
// exits and lowers parallelism. fn must only write to data owned by its idx,
// making the pool race-free without locking. progress may be nil.
func forEachCompare(host config.Host, conn remote.Client, jobs []int, progress *LoadProgressTracker, fn compareFunc) {
	if len(jobs) == 0 {
		return
	}
	workerCount := minInt(diffLoadWorkers(host), len(jobs))
	if workerCount < 1 {
		workerCount = 1
	}

	jobCh := make(chan int)
	var wg stdsync.WaitGroup
	work := func(workerConn remote.Client) {
		for idx := range jobCh {
			fn(idx, workerConn)
			if progress != nil {
				progress.Inc()
			}
		}
	}

	wg.Add(1)
	go func() {
		defer wg.Done()
		work(conn)
	}()

	for i := 1; i < workerCount; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if !isFTPProtocol(host.Protocol) {
				work(conn)
				return
			}
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()
			extraConn, err := remote.Connect(ctx, host)
			if err != nil {
				log.Debug("extra diff worker connect failed, reducing parallelism",
					"host", host.Name, "hostname", host.Hostname, "err", err)
				return
			}
			defer extraConn.Close()
			work(extraConn)
		}()
	}

	for _, idx := range jobs {
		jobCh <- idx
	}
	close(jobCh)
	wg.Wait()
}

func loadDiffItems(root *fs.Root, host config.Host, conn remote.Client, items []diffLoadItem, progress *LoadProgressTracker) []diff.Session {
	results := make([]*diff.Session, len(items))
	var jobs []int
	for i, item := range items {
		if item.Compare {
			jobs = append(jobs, i)
			continue
		}
		if item.Err != nil {
			results[i] = &diff.Session{
				LocalPath:  item.LocalPath,
				RemotePath: item.RemotePath,
				Err:        item.Err,
				Loaded:     true,
			}
		}
	}

	progress.Set("Comparing files…", 0, len(jobs), len(jobs) == 0)
	forEachCompare(host, conn, jobs, progress, func(idx int, workerConn remote.Client) {
		item := items[idx]
		result, diffErr := diff.Compare(root, item.LocalPath, item.RemotePath, workerConn)
		if diffErr != nil {
			log.Error("diff compare failed", "local", item.LocalPath, "remote", item.RemotePath, "err", diffErr)
		}
		if diffErr == nil && result != nil && !result.HasDiff() {
			return // identical — skip
		}
		results[idx] = &diff.Session{
			LocalPath:  item.LocalPath,
			RemotePath: item.RemotePath,
			Result:     result,
			Err:        diffErr,
			Loaded:     true,
		}
	})

	return sessionsFromResults(results)
}

func sessionsFromResults(results []*diff.Session) []diff.Session {
	sessions := make([]diff.Session, 0, len(results))
	for _, result := range results {
		if result != nil {
			sessions = append(sessions, *result)
		}
	}
	return sessions
}

func isFTPProtocol(protocol string) bool {
	return protocol == "ftp" || protocol == "ftps"
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func sortedMarkedPaths(sel *fs.SelectionState) []string {
	if sel == nil {
		return nil
	}
	paths := make([]string, 0, len(sel.Marked))
	for p := range sel.Marked {
		paths = append(paths, p)
	}
	sort.Strings(paths)
	return paths
}

// uploadFile streams the local file at localPath to remotePath. The local file
// is opened inside the project root, so a symlinked path component pointing out
// of the project fails the upload instead of shipping a file from outside it.
func uploadFile(conn remote.Client, root *fs.Root, localPath, remotePath string) error {
	src, err := root.Open(localPath)
	if err != nil {
		return err
	}
	defer src.Close()
	return conn.Upload(remotePath, src)
}

// downloadFile writes the remote file at remotePath to localPath inside the
// project root. WriteAtomic closes the remote stream, so a transfer that only
// fails on close never reaches the target file.
func downloadFile(conn remote.Client, root *fs.Root, remotePath, localPath string) error {
	src, err := conn.Open(remotePath)
	if err != nil {
		return err
	}
	return root.WriteAtomic(localPath, src)
}

// uploadCmd uploads the local file of sessions[idx] to remote.
func (m Model) uploadCmd(idx int) tea.Cmd {
	s := m.sessions[idx]
	conn := m.conn
	root := m.root
	tracker := m.activityTracker
	return func() tea.Msg {
		defer tracker.Finish()
		if err := uploadFile(conn, root, s.LocalPath, s.RemotePath); err != nil {
			log.Error("upload failed", "local", s.LocalPath, "remote", s.RemotePath, "err", err)
			return MsgSyncError{Err: fmt.Errorf("upload %s: %w", s.LocalPath, err)}
		}
		return MsgSynced{SessionIdx: idx, Direction: DirUpload}
	}
}

// downloadCmd downloads the remote file of sessions[idx] to local.
func (m Model) downloadCmd(idx int) tea.Cmd {
	s := m.sessions[idx]
	conn := m.conn
	root := m.root
	tracker := m.activityTracker
	return func() tea.Msg {
		defer tracker.Finish()
		if err := downloadFile(conn, root, s.RemotePath, s.LocalPath); err != nil {
			log.Error("download failed", "remote", s.RemotePath, "local", s.LocalPath, "err", err)
			return MsgSyncError{Err: fmt.Errorf("download %s: %w", s.RemotePath, err)}
		}
		return MsgSynced{SessionIdx: idx, Direction: DirDownload}
	}
}

// bulkSyncCmd executes the planned sync direction for the given session indices.
func (m Model) bulkSyncCmd(indices []int) tea.Cmd {
	sessions := m.sessions
	syncDirs := m.syncDirs
	conn := m.conn
	root := m.root
	tracker := m.syncProgress
	return func() tea.Msg {
		defer tracker.Finish()
		done := 0
		var errs []SyncFailure
		for _, i := range indices {
			if i >= len(sessions) || i >= len(syncDirs) {
				tracker.Inc()
				continue
			}
			s := sessions[i]
			var err error
			var op string
			var failurePath string
			switch syncDirs[i] {
			case DirUpload:
				op = "upload"
				failurePath = s.LocalPath
				err = uploadFile(conn, root, s.LocalPath, s.RemotePath)
			case DirDownload:
				op = "download"
				failurePath = s.RemotePath
				err = downloadFile(conn, root, s.RemotePath, s.LocalPath)
			case DirDeleteLocal:
				op = "delete local"
				failurePath = s.LocalPath
				err = root.Remove(s.LocalPath)
			case DirDeleteRemote:
				op = "delete remote"
				failurePath = s.RemotePath
				err = conn.DeleteFile(s.RemotePath)
			default:
				tracker.Inc() // DirNone — skip
				continue
			}
			if err != nil {
				log.Error("sync file", "op", op, "local", s.LocalPath, "remote", s.RemotePath, "err", err)
				reason := strings.Join(strings.Fields(err.Error()), " ")
				var protocolErr *textproto.Error
				if errors.As(err, &protocolErr) {
					reason = protocolErr.Error()
				}
				errs = append(errs, SyncFailure{
					Operation: op,
					Path:      failurePath,
					Reason:    reason,
				})
			} else {
				log.Debug("sync file ok", "op", op, "local", s.LocalPath, "remote", s.RemotePath)
				done++
			}
			tracker.Inc()
		}
		return MsgBulkSyncDone{Done: done, Errors: errs}
	}
}

// refreshCmd re-diffs all sessions in parallel using the worker pool. The
// session set (and order) is preserved so the file list stays stable.
func (m Model) refreshCmd() tea.Cmd {
	sessions := m.sessions
	host := m.host
	conn := m.conn
	root := m.root
	tracker := m.activityTracker
	return func() tea.Msg {
		defer tracker.Finish()
		refreshed := make([]diff.Session, len(sessions))
		jobs := make([]int, len(sessions))
		for i := range sessions {
			jobs[i] = i
		}
		forEachCompare(host, conn, jobs, tracker, func(idx int, workerConn remote.Client) {
			s := sessions[idx]
			result, err := diff.Compare(root, s.LocalPath, s.RemotePath, workerConn)
			if err != nil {
				log.Error("diff refresh failed", "local", s.LocalPath, "remote", s.RemotePath, "err", err)
			}
			refreshed[idx] = diff.Session{
				LocalPath:  s.LocalPath,
				RemotePath: s.RemotePath,
				Result:     result,
				Err:        err,
				Loaded:     true,
			}
		})
		return MsgRefreshed{Sessions: refreshed}
	}
}

// reloadSessionCmd recomputes one diff asynchronously after a quick sync.
func (m Model) reloadSessionCmd(idx int) tea.Cmd {
	s := m.sessions[idx]
	conn := m.conn
	root := m.root
	tracker := m.activityTracker
	return func() tea.Msg {
		defer tracker.Finish()
		result, err := diff.Compare(root, s.LocalPath, s.RemotePath, conn)
		if err != nil {
			log.Error("diff refresh failed", "local", s.LocalPath, "remote", s.RemotePath, "err", err)
		}
		tracker.Inc()
		return MsgSessionReloaded{SessionIdx: idx, Result: result, Err: err}
	}
}
