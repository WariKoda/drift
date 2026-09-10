package hostmanager

import (
	"context"
	"fmt"
	"path"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/loading"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgTestResult carries the outcome of an async connection test.
type MsgTestResult struct {
	Host config.Host
	Err  error
	ID   uint64
}

// MsgTrustReset carries the result of removing certificate trust for a host.
type MsgTrustReset struct {
	Host config.Host
	Err  error
}

// testCmd dials SSH+SFTP for host and immediately closes, returning the result.
func testCmd(host config.Host, parent context.Context, id uint64, trust *tlstrust.Manager, required *tlstrust.Challenge) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(parent, 15*time.Second)
		defer cancel()
		conn, err := remote.Connect(ctx, host, trust, required)
		if err != nil {
			log.Error("host connection test failed", "host", host.Name, "hostname", host.Hostname, "err", err)
			return MsgTestResult{Host: host, Err: err, ID: id}
		}
		root := host.RootPath
		if root == "" {
			root = "/"
		} else {
			root = path.Clean(root)
		}
		if _, err := conn.ReadDir(root); err != nil {
			_ = conn.Close()
			log.Error("host root listing failed", "host", host.Name, "remote", root, "err", err)
			return MsgTestResult{Host: host, Err: fmt.Errorf("list %s: %w", root, err), ID: id}
		}
		if err := conn.Close(); err != nil {
			return MsgTestResult{Host: host, Err: fmt.Errorf("close connection: %w", err), ID: id}
		}
		if err := conn.Err(); err != nil {
			return MsgTestResult{Host: host, Err: err, ID: id}
		}
		return MsgTestResult{Host: host, ID: id}
	}
}

// MsgOpenForm is sent when the user wants to create, edit, or duplicate a host.
type MsgOpenForm struct {
	Host      *config.Host     // nil = new host
	Scope     config.HostScope // pre-selected scope for new hosts
	OldName   string           // original name when editing
	Duplicate bool
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

	case MsgTestResult:
		if msg.ID != m.testID {
			return m, nil
		}
		m.testing = false
		m.testTracker = nil
		if loading.IsCanceled(msg.Err) {
			m.statusMsg = "Cancelled"
		} else if msg.Err != nil {
			m.statusMsg = fmt.Sprintf("✗ %s: %s", msg.Host.Name, msg.Err.Error())
		} else {
			m.statusMsg = fmt.Sprintf("✓ %s: connection successful", msg.Host.Name)
		}

	case MsgTrustReset:
		if msg.Err != nil {
			m.statusMsg = fmt.Sprintf("✗ %s: reset certificate trust: %s", msg.Host.Name, msg.Err)
		} else {
			m.statusMsg = fmt.Sprintf("✓ %s: certificate trust reset", msg.Host.Name)
		}

	case tea.MouseMsg:
		return m.updateMouse(msg)

	case tea.KeyMsg:
		if m.confirmDelete {
			return m.updateConfirm(msg)
		}
		if m.confirmReset {
			return m.updateResetConfirm(msg)
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

	case "c":
		e := m.currentEntry()
		if e == nil {
			break
		}
		names := make(map[string]bool)
		for _, candidate := range m.entries {
			if !candidate.isHeader && candidate.scope == e.scope {
				names[candidate.host.Name] = true
			}
		}
		h := e.host
		h.Name = e.host.Name + "-copy"
		for suffix := 2; names[h.Name]; suffix++ {
			h.Name = fmt.Sprintf("%s-copy-%d", e.host.Name, suffix)
		}
		return m, func() tea.Msg {
			return MsgOpenForm{Host: &h, Scope: e.scope, Duplicate: true}
		}

	case "d", "delete":
		if m.currentEntry() == nil {
			break
		}
		m.confirmDelete = true

	case "r":
		if e := m.currentEntry(); e != nil && e.host.Protocol == "ftps" {
			m.confirmReset = true
		}

	case "t":
		e := m.currentEntry()
		if e != nil && !m.testing {
			return m, m.startTest(e.host, nil)
		}

	case "esc", "q":
		return m, func() tea.Msg { return MsgBackToBrowser{} }
	}

	return m, nil
}

// RetryTest repeats a failed test while pinning the certificate shown in the
// trust prompt for the first reconnect.
func (m *Model) RetryTest(host config.Host, challenge tlstrust.Challenge) tea.Cmd {
	return m.startTest(host, &challenge)
}

func (m *Model) startTest(host config.Host, required *tlstrust.Challenge) tea.Cmd {
	m.testing = true
	m.testTarget = host.Name
	m.testID++
	id := m.testID
	m.testTracker = loading.NewTracker("Testing " + host.Name + "…")
	m.statusMsg = ""
	return testCmd(host, m.testTracker.Context(), id, m.trust, required)
}

func (m Model) updateResetConfirm(msg tea.KeyMsg) (Model, tea.Cmd) {
	if msg.String() != "y" && msg.String() != "enter" {
		m.confirmReset = false
		return m, nil
	}
	e := m.currentEntry()
	m.confirmReset = false
	if e == nil || e.host.Protocol != "ftps" || m.trust == nil {
		return m, nil
	}
	host := e.host
	trust := m.trust
	return m, func() tea.Msg {
		endpoint, err := tlstrust.NormalizeEndpoint(host.Protocol, host.Hostname, host.Port)
		if err == nil {
			err = trust.Reset(endpoint)
		}
		return MsgTrustReset{Host: host, Err: err}
	}
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
