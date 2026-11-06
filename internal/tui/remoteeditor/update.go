package remoteeditor

import (
	"context"
	"errors"
	"fmt"
	"io"
	"slices"
	"time"

	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/remoteedit"
	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
)

const operationTimeout = 60 * time.Second

func (m *Model) begin(kind operation, required *tlstrust.Challenge) tea.Cmd {
	m.seq++
	m.kind = kind
	m.busy = kind != process
	m.editing = kind == process
	if kind == process || kind == reload {
		m.snapshotReady, m.canSave = false, false
	}
	m.confirm = ""
	m.err = nil
	labels := map[operation]string{
		prepare: "Downloading remote working copy…", process: "Close the file in your editor to continue",
		capture: "Checking the working copy…", save: "Checking remote content and uploading…",
		reload: "Reloading remote content…", reconnect: "Reconnecting (no upload)…", review: "Checking remote content (no upload)…",
	}
	m.status = labels[kind]
	m.tracker = progress.NewTracker(m.status)
	request := MsgResult{Session: m.session, ID: m.seq, kind: kind}
	conn, editor, file, tracker, trust := m.conn, m.editor, m.file, m.tracker, m.trust
	if kind == reconnect {
		m.conn = nil
	}
	return func() tea.Msg {
		if kind == process {
			cmd, err := remoteedit.Command(editor, file)
			if err != nil {
				request.Result.Err = err
				return request
			}
			if editor.UsesTerminal() {
				return tea.ExecProcess(cmd, func(err error) tea.Msg {
					request.Result.Err = err
					return request
				})()
			}
			cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
			request.Result.Err = cmd.Run()
			return request
		}
		ctx, cancel := context.WithTimeout(tracker.Context(), operationTimeout)
		defer cancel()
		switch kind {
		case prepare:
			request.Result = request.Session.Prepare(ctx, conn)
		case capture:
			request.Result = request.Session.Capture(ctx)
		case save:
			request.Result = request.Session.Save(ctx, conn, tracker)
		case reload:
			request.Result = request.Session.Reload(ctx, conn)
		case review:
			request.Result = request.Session.Review(ctx, conn)
		case reconnect:
			if conn != nil {
				if err := conn.Close(); err != nil {
					log.Error("close remote editor connection before reconnect", "err", err)
					request.Result.Err = fmt.Errorf("close previous connection before reconnect: %w", err)
					return request
				}
			}
			request.Conn, request.Result.Err = request.Session.Reconnect(ctx, trust, required)
		}
		return request
	}
}

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case MsgResult:
		if !m.Accepts(msg) {
			return m, msg.Discard(m.session, m.conn)
		}
		m.busy, m.editing = false, false
		m.tracker = nil
		if msg.Result.File != "" {
			m.file = msg.Result.File
		}
		m.err = msg.Result.Err
		if msg.Result.Saved {
			m.saved, m.info = true, msg.Result.Info
		}
		if m.err != nil {
			m.canSave = false
			m.unknown = m.unknown || msg.Result.Unknown
			m.conflict = errors.Is(m.err, remoteedit.ErrConflict)
			if msg.kind == review && m.conflict {
				m.unknown = false
			}
			if msg.kind == process {
				m.status = "Editor failed. Your working copy is kept; open it again or keep the local copy."
				// Editor stderr may contain file contents. Log only the process error.
				log.Error("external editor failed", "host", m.host.Name, "err", m.err)
			} else if m.unknown {
				m.status = "Save result unknown. Reconnect and check; no automatic upload will be attempted."
			} else {
				m.status = "Operation failed. Your working copy is kept."
			}
			return m, nil
		}
		switch msg.kind {
		case prepare, reload:
			m.ready = true
			m.unknown, m.conflict = false, false
			m.canSave, m.changed = false, false
			return m, m.begin(process, nil)
		case process:
			return m, m.begin(capture, nil)
		case capture:
			m.snapshotReady = true
			m.changed, m.canSave = msg.Result.Changed, msg.Result.Changed
			if !m.changed {
				return m, m.leave(false, "No changes; nothing uploaded")
			}
			m.status = "Working copy changed. Upload requires your explicit confirmation."
		case save:
			m.saved, m.info = true, msg.Result.Info
			m.unknown, m.conflict = false, false
			if !msg.Result.Changed {
				return m, m.leave(false, "Remote file saved")
			}
			m.canSave = false
			m.status = "Remote snapshot saved, but the working copy changed again. Edit again or keep it."
		case reconnect:
			if !m.session.TakeConnection(msg.Conn) {
				m.err = errors.New("reconnect result no longer belongs to this session")
				return m, msg.Discard(m.session, m.conn)
			}
			m.conn = msg.Conn
			if !m.ready {
				return m, m.begin(prepare, nil)
			}
			return m, m.begin(review, nil)
		case review:
			m.unknown, m.conflict = false, false
			if msg.Result.Saved {
				if !msg.Result.Changed {
					return m, m.leave(false, "Remote content verified; no retry needed")
				}
				m.snapshotReady = false
			}
			m.canSave = m.snapshotReady && m.changed
			if m.canSave {
				m.status = "Reconnected and checked. Upload still requires explicit confirmation."
			} else {
				m.status = "Reconnected and checked. Open the editor again before uploading."
			}
		}
	case tea.KeyMsg:
		key := msg.String()
		if m.busy {
			if key == "esc" || key == "q" || key == "ctrl+c" {
				m.CancelActivity()
			}
			return m, nil
		}
		if m.editing {
			m.status = "Close the file in your editor to continue; the working copy will be kept."
			return m, nil
		}
		if m.confirm != "" {
			if key == "y" {
				switch m.confirm {
				case "discard":
					return m, m.leave(false, "Working copy discarded")
				case "reload":
					return m, m.begin(reload, nil)
				}
			}
			if m.confirm == "leave" {
				if key == "k" {
					return m, m.leave(true, "Working copy kept at "+m.file)
				}
				if key == "d" {
					m.confirm = "discard"
					return m, nil
				}
			}
			if key == "esc" || key == "n" || key == "q" || key == "ctrl+c" {
				m.confirm = ""
			}
			return m, nil
		}
		switch key {
		case "u":
			if m.canSave && !m.unknown && !m.conflict && usable(m.conn) {
				return m, m.begin(save, nil)
			}
		case "e", "enter":
			if m.ready && !m.unknown {
				m.canSave = false
				return m, m.begin(process, nil)
			}
		case "c":
			return m, m.begin(reconnect, nil)
		case "r":
			if m.ready && usable(m.conn) {
				m.confirm = "reload"
			}
		case "k":
			return m, m.leave(true, "Working copy kept at "+m.file)
		case "d":
			m.confirm = "discard"
		case "q", "ctrl+c", "esc":
			m.confirm = "leave"
		}
	}
	return m, nil
}

func (m *Model) leave(keep bool, status string) tea.Cmd {
	m.seq++
	m.kind, m.busy, m.canSave = finish, true, false
	m.status = "Closing editor session…"
	result := MsgClosed{Session: m.session, ID: m.seq, Conn: m.conn, Info: m.info, Saved: m.saved, Status: status, File: m.file}
	m.conn = nil
	discard := m.confirm == "discard"
	return func() tea.Msg {
		if keep {
			result.Err = result.Session.Close(false)
		} else if discard {
			result.Err = result.Session.Close(true)
		} else {
			result.Err = result.Session.Complete()
		}
		if result.Conn != nil && !usable(result.Conn) {
			result.Err = errors.Join(result.Err, result.Conn.Close())
			result.Conn = nil
		}
		return result
	}
}

func usable(conn remote.Client) bool {
	if conn == nil || conn.Err() != nil {
		return false
	}
	select {
	case <-conn.Done():
		return false
	default:
		return true
	}
}

func (msg MsgResult) Discard(current *remoteedit.Session, live ...remote.Client) tea.Cmd {
	return func() tea.Msg {
		var err error
		if msg.Conn != nil && !slices.Contains(live, msg.Conn) {
			if msg.Session != nil {
				msg.Session.TakeConnection(msg.Conn)
			}
			err = msg.Conn.Close()
		}
		if msg.Session != nil && msg.Session != current {
			err = errors.Join(err, msg.Session.Close(false))
		}
		if err != nil {
			log.Error("close stale remote editor result", "err", err)
		}
		return nil
	}
}

func (msg MsgClosed) Discard(live ...remote.Client) tea.Cmd {
	return func() tea.Msg {
		if msg.Conn != nil && !slices.Contains(live, msg.Conn) {
			if err := msg.Conn.Close(); err != nil {
				log.Error("close stale remote editor return", "err", err)
			}
		}
		return nil
	}
}

func (m Model) Error() error { return m.err }
