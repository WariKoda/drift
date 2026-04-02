// Package diffview implements the split-pane local/remote diff screen.
package diffview

import (
	"context"
	"fmt"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/yourusername/drift/internal/config"
	"github.com/yourusername/drift/internal/diff"
	"github.com/yourusername/drift/internal/fs"
	"github.com/yourusername/drift/internal/pathmap"
	"github.com/yourusername/drift/internal/sftp"
)

// MsgDiffLoaded is sent when all sessions have been computed.
// Conn is kept open for subsequent sync operations — caller must close it.
type MsgDiffLoaded struct {
	Sessions []diff.Session
	Conn     *sftp.Client
}

// MsgDiffError is sent when SSH/SFTP connection or diff loading fails.
type MsgDiffError struct{ Err error }

// MsgSynced is sent after a successful upload or download.
type MsgSynced struct{ SessionIdx int; Direction SyncDir }

// MsgSyncError is sent when a sync operation fails.
type MsgSyncError struct{ Err error }

// SyncDir indicates upload or download.
type SyncDir int

const (
	DirUpload   SyncDir = iota
	DirDownload
)

// Model is the diff view screen.
type Model struct {
	sessions  []diff.Session
	activeIdx int
	scroll    int
	host      config.Host
	conn      *sftp.Client // kept open for sync ops
	Width     int
	Height    int
}

// New creates a Model with pre-loaded sessions.
func New(sessions []diff.Session, host config.Host, conn *sftp.Client, width, height int) Model {
	return Model{
		sessions: sessions,
		host:     host,
		conn:     conn,
		Width:    width,
		Height:   height,
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

// viewportHeight returns available lines for diff content.
func (m Model) viewportHeight() int {
	h := m.Height - 6 // header + sep + col headers + sep + status + sep
	if h < 1 {
		return 1
	}
	return h
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
func LoadCmd(host config.Host, sel *fs.SelectionState, cfg *config.MergedConfig) tea.Cmd {
	return func() tea.Msg {
		conn, err := sftp.Connect(context.Background(), host)
		if err != nil {
			return MsgDiffError{Err: fmt.Errorf("connect to %s: %w", host.Hostname, err)}
		}

		mapper := pathmap.New(cfg.ProjectRoot, cfg.Mappings, host)
		var sessions []diff.Session

		for localPath := range sel.Marked {
			remotePath, mapErr := mapper.LocalToRemote(localPath)
			if mapErr != nil {
				sessions = append(sessions, diff.Session{
					LocalPath: localPath, Err: mapErr, Loaded: true,
				})
				continue
			}
			result, diffErr := diff.Compare(localPath, remotePath, conn)
			sessions = append(sessions, diff.Session{
				LocalPath:  localPath,
				RemotePath: remotePath,
				Result:     result,
				Err:        diffErr,
				Loaded:     true,
			})
		}

		// conn stays open — caller (App) passes it to diffview.New for sync ops
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

// reloadSession recomputes the diff for sessions[idx] after a sync.
func (m *Model) reloadSession(idx int) {
	s := &m.sessions[idx]
	result, err := diff.Compare(s.LocalPath, s.RemotePath, m.conn)
	s.Result = result
	s.Err = err
}
