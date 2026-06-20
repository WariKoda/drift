package hostform

import (
	"github.com/WariKoda/drift/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgHostSaved is emitted when the user saves a valid host form.
type MsgHostSaved struct {
	Host    config.Host
	Scope   config.HostScope
	IsEdit  bool
	OldName string
}

// MsgFormCancelled is emitted when the user presses Esc on the main form.
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
	switch m.sub {
	case subMappingList:
		return m.handleMappingList(msg)
	case subMappingEdit:
		return m.handleMappingEdit(msg)
	default:
		return m.handleMainKey(msg)
	}
}

// ── Main form ─────────────────────────────────────────────────────────────────

func (m Model) handleMainKey(msg tea.KeyMsg) (Model, tea.Cmd) {
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
		if curIdx == fMappings {
			m.sub = subMappingList
			return m, nil
		}
		if curIdx == fProtocol {
			m.protocol = (m.protocol + 1) % 3
			m.applyFocus()
		} else if curIdx == fAuthType {
			m.authType = (m.authType + 1) % 3
			m.applyFocus()
		} else if curIdx == fInsecureTLS {
			m.insecureTLS = !m.insecureTLS
			m.applyFocus()
		} else {
			m.focusRow++
			if m.focusRow >= len(rows) {
				return m.trySave()
			}
			m.applyFocus()
		}

	case "ctrl+s":
		return m.trySave()

	case " ":
		if curIdx == fProtocol {
			m.protocol = (m.protocol + 1) % 3
			m.applyFocus()
		} else if curIdx == fAuthType {
			m.authType = (m.authType + 1) % 3
			m.applyFocus()
		} else if curIdx == fInsecureTLS {
			m.insecureTLS = !m.insecureTLS
			m.applyFocus()
		} else if curIdx == fScope {
			if m.scope == config.ScopeGlobal {
				m.scope = config.ScopeProject
			} else {
				m.scope = config.ScopeGlobal
			}
		} else if curIdx == fMappings {
			m.sub = subMappingList
			return m, nil
		} else {
			if curIdx >= 0 && curIdx < len(m.fields) && m.fields[curIdx] != nil {
				m.fields[curIdx].HandleKey(msg)
			}
		}

	case "left", "right":
		if curIdx == fProtocol {
			m.protocol = (m.protocol + 1) % 3
			m.applyFocus()
		} else if curIdx == fAuthType {
			if msg.String() == "right" {
				m.authType = (m.authType + 1) % 3
			} else {
				m.authType = (m.authType + 2) % 3
			}
			m.applyFocus()
		} else if curIdx == fInsecureTLS {
			m.insecureTLS = !m.insecureTLS
			m.applyFocus()
		} else if curIdx == fScope {
			if m.scope == config.ScopeGlobal {
				m.scope = config.ScopeProject
			} else {
				m.scope = config.ScopeGlobal
			}
		} else {
			if curIdx >= 0 && curIdx < len(m.fields) && m.fields[curIdx] != nil {
				m.fields[curIdx].HandleKey(msg)
			}
		}

	default:
		if curIdx >= 0 && curIdx < len(m.fields) && m.fields[curIdx] != nil {
			m.fields[curIdx].HandleKey(msg)
		}
	}

	return m, nil
}

// ── Mapping list ──────────────────────────────────────────────────────────────

func (m Model) handleMappingList(msg tea.KeyMsg) (Model, tea.Cmd) {
	if m.mapConfirmDel {
		switch msg.String() {
		case "y":
			if m.mapCursor < len(m.mappings) {
				m.mappings = append(m.mappings[:m.mapCursor], m.mappings[m.mapCursor+1:]...)
				if m.mapCursor >= len(m.mappings) && m.mapCursor > 0 {
					m.mapCursor--
				}
			}
			m.mapConfirmDel = false
		default:
			m.mapConfirmDel = false
		}
		return m, nil
	}

	switch msg.String() {
	case "esc":
		m.sub = subMain
		m.applyFocus()

	case "j", "down":
		if m.mapCursor < len(m.mappings)-1 {
			m.mapCursor++
		}

	case "k", "up":
		if m.mapCursor > 0 {
			m.mapCursor--
		}

	case "n":
		m.openMappingEdit(-1)

	case "e", "enter":
		if len(m.mappings) > 0 {
			m.openMappingEdit(m.mapCursor)
		} else {
			m.openMappingEdit(-1)
		}

	case "d", "delete":
		if len(m.mappings) > 0 {
			m.mapConfirmDel = true
		}
	}

	return m, nil
}

// ── Mapping edit ──────────────────────────────────────────────────────────────

func (m Model) handleMappingEdit(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case "esc":
		m.sub = subMappingList

	case "ctrl+s":
		m.saveMappingEdit()
		m.sub = subMappingList

	case "tab", "down":
		m.editFocusRow++
		if m.editFocusRow >= 2 {
			m.editFocusRow = 0
		}
		m.applyEditFocus()

	case "shift+tab", "up":
		m.editFocusRow--
		if m.editFocusRow < 0 {
			m.editFocusRow = 1
		}
		m.applyEditFocus()

	case "enter":
		if m.editFocusRow == 1 {
			m.saveMappingEdit()
			m.sub = subMappingList
		} else {
			m.editFocusRow++
			m.applyEditFocus()
		}

	default:
		if m.editFocusRow >= 0 && m.editFocusRow < 2 && m.editFields[m.editFocusRow] != nil {
			m.editFields[m.editFocusRow].HandleKey(msg)
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
