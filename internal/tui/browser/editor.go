package browser

import (
	"os"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
)

type MsgEditRequested struct {
	Host       config.Host
	Path       string
	Conn       remote.Client
	token      *string
	session    *string
	generation uint64
}

func (m *Model) requestEdit() tea.Cmd {
	if m.activePane != PaneRemote {
		m.statusMsg = "Remote editing is available in the remote pane only"
		return nil
	}
	if m.remoteBusy() {
		m.statusMsg = "Wait for the remote operation to finish"
		return nil
	}
	if m.remoteHost == nil || m.remoteConn == nil || m.remoteConn.Err() != nil {
		m.statusMsg = "Connect to a remote host before editing"
		return nil
	}
	select {
	case <-m.remoteConn.Done():
		m.statusMsg = "Remote disconnected. Reconnect with [r] before editing."
		return nil
	default:
	}
	entry := m.remoteCurrent()
	if entry == nil || entry.Kind != fs.EntryFile || !entry.Mode.IsRegular() {
		m.statusMsg = "Select a regular remote file to edit"
		return nil
	}
	if entry.Unmapped || entry.Class.HardExcluded {
		m.statusMsg = "Path is outside the active mappings or excluded"
		return nil
	}
	var editor *config.Editor
	if m.config != nil {
		editor = m.config.Editor
	}
	if err := editor.Validate(); err != nil {
		m.statusMsg = err.Error()
		return nil
	}
	token := entry.Path
	m.editToken = &token
	m.editPending = true
	m.editConn, m.remoteConn = m.remoteConn, nil
	host := *m.remoteHost
	host.Mappings = append([]config.Mapping(nil), host.Mappings...)
	request := MsgEditRequested{Host: host, Path: entry.Path, Conn: m.editConn,
		token: m.editToken, session: m.remoteSession, generation: m.remoteLoadID}
	m.preview.generation++
	m.preview.loading, m.preview.waiting = false, false
	cmd := tea.Cmd(func() tea.Msg { return request })
	if m.preview.active && m.mouseEnabled {
		return tea.Sequence(tea.EnableMouseCellMotion, cmd)
	}
	return cmd
}

func (m *Model) TakeEditRequest(request MsgEditRequested) bool {
	if !m.editPending || request.token != m.editToken || request.session != m.remoteSession ||
		request.generation != m.remoteLoadID || request.Conn != m.editConn {
		return false
	}
	m.editPending = false
	m.editConn = nil
	return true
}

func (m *Model) ResumeEdit(request MsgEditRequested, conn remote.Client, info os.FileInfo, saved bool, status string) tea.Cmd {
	if request.token != m.editToken || request.session != m.remoteSession || request.generation != m.remoteLoadID {
		return nil
	}
	m.editToken = nil
	m.remoteConn = conn
	m.remoteLoadID++
	m.statusMsg = status
	if conn == nil {
		m.remoteStatus = "Remote disconnected. Press [r] to reconnect."
	} else {
		m.remoteStatus = "Remote connected: " + request.Host.Name
	}
	if info != nil {
		if i := m.remoteIndexByPath(request.Path); i >= 0 {
			entry := m.remoteEntries[i]
			entry.Size, entry.ModTime, entry.Mode = info.Size(), info.ModTime(), info.Mode()
		}
	}
	var previewCmd tea.Cmd
	if saved && m.preview.active && m.preview.source == PaneRemote && m.preview.path == request.Path {
		m.preview.loaded = false
		m.preview.lines, m.preview.rows = nil, nil
		m.preview.message = "Loading updated remote preview…"
		previewCmd = m.schedulePreview()
	}
	if m.preview.active && m.mouseEnabled {
		return tea.Batch(tea.DisableMouse, previewCmd)
	}
	return previewCmd
}
