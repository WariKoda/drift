// Package remoteeditor presents an external-editor session and upload decisions.
package remoteeditor

import (
	"errors"
	"os"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/remoteedit"
	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
)

type operation int

const (
	prepare operation = iota
	process
	capture
	save
	reload
	reconnect
	review
	finish
)

// MsgResult carries the immutable identity of the operation which produced it.
type MsgResult struct {
	Session *remoteedit.Session
	ID      uint64
	kind    operation
	Result  remoteedit.Result
	Conn    remote.Client // only a newly established reconnect transport
}

type MsgClosed struct {
	Session *remoteedit.Session
	ID      uint64
	Conn    remote.Client
	Info    os.FileInfo
	Saved   bool
	Status  string
	File    string
	Err     error
}

type Model struct {
	session       *remoteedit.Session
	host          config.Host
	path          string
	file          string
	editor        *config.Editor
	conn          remote.Client
	trust         *tlstrust.Manager
	width         int
	height        int
	seq           uint64
	kind          operation
	busy          bool
	editing       bool
	ready         bool
	changed       bool
	canSave       bool
	snapshotReady bool
	unknown       bool
	conflict      bool
	saved         bool
	info          os.FileInfo
	confirm       string
	status        string
	err           error
	tracker       *progress.Tracker
}

func New(request remoteedit.Request, editor *config.Editor, conn remote.Client, trust *tlstrust.Manager, width, height int) Model {
	var copyEditor *config.Editor
	if editor != nil {
		value := *editor
		value.Command = append([]string(nil), editor.Command...)
		copyEditor = &value
	}
	return Model{
		session: remoteedit.New(request), host: request.Host, path: request.RemotePath,
		editor: copyEditor, conn: conn, trust: trust, width: width, height: height,
	}
}

func (m *Model) Init() tea.Cmd { return m.begin(prepare, nil) }

func (m *Model) SetSize(width, height int) { m.width, m.height = width, height }

func (m Model) Connection() remote.Client { return m.conn }

func (m Model) Accepts(msg MsgResult) bool {
	return msg.Session == m.session && msg.ID == m.seq && msg.kind == m.kind
}

func (m Model) AcceptsClosed(msg MsgClosed) bool {
	return msg.Session == m.session && msg.ID == m.seq && m.kind == finish
}

func (m Model) LoadingActivity() (string, *progress.Tracker, bool) {
	return m.status, m.tracker, m.busy && m.kind != finish
}

func (m *Model) CancelActivity() {
	if m.tracker != nil {
		m.tracker.Cancel()
	}
	m.status = "Cancellation requested; waiting for the operation result"
}

func (m *Model) ConnectionLost(conn remote.Client, err error) bool {
	if conn == nil || conn != m.conn || err == nil {
		return false
	}
	// Do not clear the candidate or interrupt local editing. The command's
	// result still decides whether a write was confirmed or has an unknown outcome.
	m.err = err
	m.status = "Remote disconnected. Keep your copy or reconnect and check before uploading."
	return true
}

func (m *Model) RetryConnection(challenge tlstrust.Challenge) tea.Cmd {
	return m.begin(reconnect, &challenge)
}

// Close is used after Bubble Tea exits. Unconfirmed working data is always kept.
func (m *Model) Close() tea.Cmd {
	if m.session == nil {
		return nil
	}
	m.CancelActivity()
	session, conn := m.session, m.conn
	m.conn = nil
	return func() tea.Msg {
		err := session.Close(false)
		if conn != nil {
			err = errors.Join(err, conn.Close())
		}
		if err != nil {
			log.Error("close remote editor on shutdown", "err", err)
		}
		return nil
	}
}

func (m Model) Host() config.Host { return m.host }
