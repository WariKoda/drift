package hostmanager

import (
	"context"
	"fmt"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/nibra180/drift-tui/internal/config"
	"github.com/nibra180/drift-tui/internal/remote"
)

// msgTestResult carries the outcome of an async connection test.
type msgTestResult struct {
	hostName string
	err      error
}

// testCmd dials SSH+SFTP for host and immediately closes, returning the result.
func testCmd(host config.Host) tea.Cmd {
	return func() tea.Msg {
		conn, err := remote.Connect(context.Background(), host)
		if err != nil {
			return msgTestResult{hostName: host.Name, err: err}
		}
		conn.Close()
		return msgTestResult{hostName: host.Name}
	}
}

// MsgOpenForm is sent when the user wants to create or edit a host.
type MsgOpenForm struct {
	Host    *config.Host     // nil = new host
	Scope   config.HostScope // pre-selected scope for new hosts
	OldName string           // original name when editing
}

// MsgDeleteHost is sent when a delete is confirmed.
type MsgDeleteHost struct {
	Name  string
	Scope config.HostScope
}

// MsgBackToBrowser is sent when the user presses Esc.
type MsgBackToBrowser struct{}

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height

	case msgTestResult:
		m.testing = false
		if msg.err != nil {
			m.statusMsg = fmt.Sprintf("✗ %s: %s", msg.hostName, msg.err.Error())
		} else {
			m.statusMsg = fmt.Sprintf("✓ %s: connection successful", msg.hostName)
		}

	case tea.KeyMsg:
		if m.confirmDelete {
			return m.updateConfirm(msg)
		}
		return m.updateNormal(msg)
	}
	return m, nil
}

func (m Model) updateNormal(msg tea.KeyMsg) (Model, tea.Cmd) {
	m.statusMsg = ""

	switch msg.String() {
	case "j", "down":
		m.cursor++
		m.clampCursor()

	case "k", "up":
		m.cursor--
		m.clampCursorUp()

	case "g":
		m.cursor = 0
		m.clampCursor()

	case "G":
		m.cursor = len(m.entries) - 1
		m.clampCursor()

	case "n":
		// Determine scope from cursor position
		scope := config.ScopeGlobal
		if e := m.currentEntry(); e != nil {
			scope = e.scope
		} else if m.cursor >= 0 && m.cursor < len(m.entries) && m.entries[m.cursor].isHeader {
			scope = m.entries[m.cursor].scope
		}
		return m, func() tea.Msg {
			return MsgOpenForm{Scope: scope}
		}

	case "e", "enter":
		e := m.currentEntry()
		if e == nil {
			break
		}
		h := e.host
		return m, func() tea.Msg {
			return MsgOpenForm{Host: &h, Scope: e.scope, OldName: h.Name}
		}

	case "d", "delete":
		if m.currentEntry() == nil {
			break
		}
		m.confirmDelete = true

	case "t":
		e := m.currentEntry()
		if e != nil && !m.testing {
			m.testing = true
			m.testTarget = e.host.Name
			m.statusMsg = ""
			return m, testCmd(e.host)
		}

	case "esc", "q":
		return m, func() tea.Msg { return MsgBackToBrowser{} }
	}

	return m, nil
}

func (m Model) updateConfirm(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case "y", "enter":
		e := m.currentEntry()
		if e == nil {
			m.confirmDelete = false
			break
		}
		name := e.host.Name
		scope := e.scope
		m.confirmDelete = false
		return m, func() tea.Msg {
			return MsgDeleteHost{Name: name, Scope: scope}
		}

	default: // any other key cancels
		m.confirmDelete = false
	}
	return m, nil
}

// Refresh rebuilds the entry list after an external config change.
func (m *Model) Refresh() {
	m.rebuild()
}
