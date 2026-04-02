package hostform

import (
	tea "github.com/charmbracelet/bubbletea"
	"github.com/nibra180/drift-tui/internal/config"
)

// MsgHostSaved is emitted when the user saves a valid host form.
type MsgHostSaved struct {
	Host    config.Host
	Scope   config.HostScope
	IsEdit  bool
	OldName string
}

// MsgFormCancelled is emitted when the user presses Esc.
type MsgFormCancelled struct{}

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.SetSize(msg.Width, msg.Height)

	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	rows := m.visibleRows()
	curIdx := -1
	if m.focusRow >= 0 && m.focusRow < len(rows) {
		curIdx = rows[m.focusRow]
	}

	switch msg.String() {
	case "esc":
		return m, func() tea.Msg { return MsgFormCancelled{} }

	case "tab", "down":
		m.focusRow++
		if m.focusRow >= len(rows) {
			m.focusRow = 0
		}
		m.applyFocus()
		m.errMsg = ""

	case "shift+tab", "up":
		m.focusRow--
		if m.focusRow < 0 {
			m.focusRow = len(rows) - 1
		}
		m.applyFocus()
		m.errMsg = ""

	case "enter":
		if curIdx == fScope {
			return m.trySave()
		}
		if curIdx == fProtocol {
			m.protocol = (m.protocol + 1) % 2
			m.applyFocus()
		} else if curIdx == fAuthType {
			m.authType = (m.authType + 1) % 3
			m.applyFocus()
		} else {
			// Move to next field
			m.focusRow++
			if m.focusRow >= len(rows) {
				return m.trySave()
			}
			m.applyFocus()
		}

	case "ctrl+s":
		return m.trySave()

	case " ":
		// Space toggles on toggle rows
		if curIdx == fProtocol {
			m.protocol = (m.protocol + 1) % 2
			m.applyFocus()
		} else if curIdx == fAuthType {
			m.authType = (m.authType + 1) % 3
			m.applyFocus()
		} else if curIdx == fScope {
			if m.scope == config.ScopeGlobal {
				m.scope = config.ScopeProject
			} else {
				m.scope = config.ScopeGlobal
			}
		} else {
			// Pass space to text field
			if curIdx >= 0 && curIdx < len(m.fields) && m.fields[curIdx] != nil {
				m.fields[curIdx].HandleKey(msg)
			}
		}

	case "left", "right":
		// On toggle rows, left/right cycle options
		if curIdx == fProtocol {
			m.protocol = (m.protocol + 1) % 2
			m.applyFocus()
		} else if curIdx == fAuthType {
			if msg.String() == "right" {
				m.authType = (m.authType + 1) % 3
			} else {
				m.authType = (m.authType + 2) % 3 // -1 mod 3
			}
			m.applyFocus()
		} else if curIdx == fScope {
			if m.scope == config.ScopeGlobal {
				m.scope = config.ScopeProject
			} else {
				m.scope = config.ScopeGlobal
			}
		} else {
			// pass to text field
			if curIdx >= 0 && curIdx < len(m.fields) && m.fields[curIdx] != nil {
				m.fields[curIdx].HandleKey(msg)
			}
		}

	default:
		// Delegate to focused text field
		if curIdx >= 0 && curIdx < len(m.fields) && m.fields[curIdx] != nil {
			m.fields[curIdx].HandleKey(msg)
		}
	}

	return m, nil
}

func (m Model) trySave() (Model, tea.Cmd) {
	h, err := m.toHost()
	if err != nil {
		m.errMsg = err.Error()
		return m, nil
	}
	m.errMsg = ""
	saved := MsgHostSaved{
		Host:    h,
		Scope:   m.scope,
		IsEdit:  m.isEdit,
		OldName: m.oldName,
	}
	return m, func() tea.Msg { return saved }
}
