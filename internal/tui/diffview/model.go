// Package diffview implements the split-pane local/remote diff screen.
package diffview

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/pathmap"
	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgDiffLoaded is sent when all sessions have been computed.
// Conn is kept open for subsequent sync operations — caller must close it.
type MsgDiffLoaded struct {
	Sessions []diff.Session
	Conn     remote.Client
}

// MsgDiffError is sent when SSH/SFTP connection or diff loading fails.
type MsgDiffError struct{ Err error }

// MsgRefreshed is sent when a full diff refresh has completed.
type MsgRefreshed struct{ Sessions []diff.Session }

// MsgBulkSyncDone is sent when bulk sync has finished.
type MsgBulkSyncDone struct {
	Done   int      // number of successfully synced files
	Errors []string // one entry per failed file
}

// MsgSynced is sent after a successful upload or download.
type MsgSynced struct {
	SessionIdx int
	Direction  SyncDir
}

// MsgSyncError is sent when a sync operation fails.
type MsgSyncError struct{ Err error }

// SyncDir indicates the planned sync direction for a file.
type SyncDir int

const (
	DirNone         SyncDir = iota // — no sync planned (zero value / default)
	DirUpload                      // ↑ push local → remote
	DirDownload                    // ↓ pull remote → local
	DirDeleteLocal                 // ✗ delete the local file
	DirDeleteRemote                // ✗ delete the remote file
)

// nextDir returns the next direction in the cycle for s's file-existence state.
//
//	Both sides exist : None → Upload → Download → None
//	Local only       : None → Upload → DeleteLocal → None
//	Remote only      : None → Download → DeleteRemote → None
func nextDir(cur SyncDir, s *diff.Session) SyncDir {
	if s.Err != nil || s.Result == nil {
		return DirNone
	}
	switch {
	case s.Result.LocalOnly:
		switch cur {
		case DirNone:
			return DirUpload
		case DirUpload:
			return DirDeleteLocal
		default:
			return DirNone
		}
	case s.Result.RemoteOnly:
		switch cur {
		case DirNone:
			return DirDownload
		case DirDownload:
			return DirDeleteRemote
		default:
			return DirNone
		}
	default:
		switch cur {
		case DirNone:
			return DirUpload
		case DirUpload:
			return DirDownload
		default:
			return DirNone
		}
	}
}

// Model is the diff view screen.
type Model struct {
	sessions       []diff.Session
	syncDirs       []SyncDir // planned direction per session (index-aligned)
	activeIdx      int
	fileListOffset int // scroll offset into the file list
	scroll         int
	refreshing     bool   // true while async refresh is in flight
	syncing        bool   // true while bulk sync is in flight
	syncStatus     string // last bulk sync result message
	host           config.Host
	conn           remote.Client // kept open for sync ops
	Width          int
	Height         int
}

// New creates a Model with pre-loaded sessions.
// syncDirs are pre-filled by autoDir so the user starts with a sensible selection.
func New(sessions []diff.Session, host config.Host, conn remote.Client, width, height int) Model {
	syncDirs := make([]SyncDir, len(sessions))
	for i := range sessions {
		syncDirs[i] = autoDir(&sessions[i])
	}
	return Model{
		sessions: sessions,
		syncDirs: syncDirs,
		host:     host,
		conn:     conn,
		Width:    width,
		Height:   height,
	}
}

// autoDir returns the most logical sync direction for a session based on file
// existence and modification times.
//
//   - LocalOnly  → Upload   (remote is missing the file)
//   - RemoteOnly → Download (local is missing the file)
//   - Identical  → None     (nothing to do)
//   - Both differ → compare mtimes; local newer ↑, remote newer ↓, ambiguous —
//
// A 2-second threshold tolerates FAT32 time resolution and minor clock drift.
func autoDir(s *diff.Session) SyncDir {
	if s.Err != nil || s.Result == nil {
		return DirNone
	}
	r := s.Result
	switch {
	case r.LocalOnly:
		return DirUpload
	case r.RemoteOnly:
		return DirDownload
	case !r.HasDiff():
		return DirNone
	default:
		const threshold = 2 * time.Second
		delta := r.ModLocal.Sub(r.ModRemote)
		if delta > threshold {
			return DirUpload
		}
		if delta < -threshold {
			return DirDownload
		}
		return DirNone
	}
}

// Init satisfies the sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// Close closes the SFTP connection. Call when leaving the diff view.
func (m *Model) Close() {
	if m.conn != nil {
		_ = m.conn.Close()
		m.conn = nil
	}
}

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) { m.Width = w; m.Height = h }

// fileListHeight returns the number of visible rows in the file list (capped at 5).
func (m Model) fileListHeight() int {
	n := len(m.sessions)
	if n > 5 {
		n = 5
	}
	if n < 1 {
		return 1
	}
	return n
}

// viewportHeight returns available lines for diff content.
func (m Model) viewportHeight() int {
	// header(1) + sep(1) + fileList(fh) + sep(1) + colLabels(1) + sep(1) + content(vh) + sep(1) + status(1) = Height
	h := m.Height - 7 - m.fileListHeight()
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

// paneWidth returns the width of one diff pane.
func (m Model) paneWidth() int {
	w := (m.Width - 1) / 2 // 1 for │ divider
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

// LoadCmd returns a tea.Cmd that connects to host and loads all diffs asynchronously.
// Marked directories are expanded recursively; remote-only files inside those
// directories are detected by walking the remote side as well.
func LoadCmd(host config.Host, sel *fs.SelectionState, cfg *config.MergedConfig) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		conn, err := remote.Connect(ctx, host)
		if err != nil {
			return MsgDiffError{Err: fmt.Errorf("connect to %s: %w", host.Hostname, err)}
		}

		mapper := pathmap.New(cfg.ProjectRoot, cfg.Mappings, host)
		var sessions []diff.Session

		// addFile creates one diff session for a local/remote file pair.
		// Files that are identical on both sides are skipped.
		addFile := func(localPath, remotePath string) {
			result, diffErr := diff.Compare(localPath, remotePath, conn)
			if diffErr == nil && result != nil && !result.HasDiff() {
				return // identical — skip
			}
			sessions = append(sessions, diff.Session{
				LocalPath:  localPath,
				RemotePath: remotePath,
				Result:     result,
				Err:        diffErr,
				Loaded:     true,
			})
		}

		for localPath := range sel.Marked {
			info, statErr := os.Stat(localPath)
			if statErr != nil {
				sessions = append(sessions, diff.Session{
					LocalPath: localPath, Err: statErr, Loaded: true,
				})
				continue
			}

			if !info.IsDir() {
				// ── Single file ──────────────────────────────────────
				remotePath, mapErr := mapper.LocalToRemote(localPath)
				if mapErr != nil {
					sessions = append(sessions, diff.Session{
						LocalPath: localPath, Err: mapErr, Loaded: true,
					})
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
					sessions = append(sessions, diff.Session{
						LocalPath: p, Err: mapErr, Loaded: true,
					})
					return nil
				}
				addFile(p, remotePath)
				return nil
			}); walkErr != nil {
				sessions = append(sessions, diff.Session{
					LocalPath: localPath, Err: fmt.Errorf("walk local: %w", walkErr), Loaded: true,
				})
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
				sessions = append(sessions, diff.Session{
					LocalPath: localPath, Err: fmt.Errorf("walk remote: %w", walkErr), Loaded: true,
				})
			}
		}

		return MsgDiffLoaded{Sessions: sessions, Conn: conn}
	}
}

// uploadCmd uploads the local file of sessions[idx] to remote.
func (m Model) uploadCmd(idx int) tea.Cmd {
	s := m.sessions[idx]
	conn := m.conn
	return func() tea.Msg {
		if err := conn.UploadFile(s.LocalPath, s.RemotePath); err != nil {
			return MsgSyncError{Err: fmt.Errorf("upload %s: %w", s.LocalPath, err)}
		}
		return MsgSynced{SessionIdx: idx, Direction: DirUpload}
	}
}

// downloadCmd downloads the remote file of sessions[idx] to local.
func (m Model) downloadCmd(idx int) tea.Cmd {
	s := m.sessions[idx]
	conn := m.conn
	return func() tea.Msg {
		if err := conn.DownloadFile(s.RemotePath, s.LocalPath); err != nil {
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
	return func() tea.Msg {
		done := 0
		var errs []string
		for _, i := range indices {
			if i >= len(sessions) || i >= len(syncDirs) {
				continue
			}
			s := sessions[i]
			var err error
			switch syncDirs[i] {
			case DirUpload:
				err = conn.UploadFile(s.LocalPath, s.RemotePath)
			case DirDownload:
				err = conn.DownloadFile(s.RemotePath, s.LocalPath)
			case DirDeleteLocal:
				err = os.Remove(s.LocalPath)
			case DirDeleteRemote:
				err = conn.DeleteFile(s.RemotePath)
			default:
				continue // DirNone — skip
			}
			if err != nil {
				errs = append(errs, err.Error())
			} else {
				done++
			}
		}
		return MsgBulkSyncDone{Done: done, Errors: errs}
	}
}

// refreshCmd re-diffs all sessions using the existing connection.
func (m Model) refreshCmd() tea.Cmd {
	sessions := m.sessions
	conn := m.conn
	return func() tea.Msg {
		refreshed := make([]diff.Session, len(sessions))
		for i, s := range sessions {
			result, err := diff.Compare(s.LocalPath, s.RemotePath, conn)
			refreshed[i] = diff.Session{
				LocalPath:  s.LocalPath,
				RemotePath: s.RemotePath,
				Result:     result,
				Err:        err,
				Loaded:     true,
			}
		}
		return MsgRefreshed{Sessions: refreshed}
	}
}

// reloadSession recomputes the diff for sessions[idx] after a sync.
func (m *Model) reloadSession(idx int) {
	s := &m.sessions[idx]
	result, err := diff.Compare(s.LocalPath, s.RemotePath, m.conn)
	s.Result = result
	s.Err = err
}
