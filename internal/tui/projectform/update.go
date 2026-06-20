package projectform

import (
	tea "github.com/charmbracelet/bubbletea"
)

// MsgProjectSaved is emitted when the user saves a valid project form.
// OldSlug is empty for a new project; for an edit it identifies the entry to replace.
type MsgProjectSaved struct {
	Name    string
	Path    string
	OldSlug string
}

// MsgProjectFormCancelled is emitted when the user presses Esc.
type MsgProjectFormCancelled struct{}

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
	switch msg.String() {
	case "esc":
		return m, func() tea.Msg { return MsgProjectFormCancelled{} }

	case "tab", "down":
		m.focusRow = (m.focusRow + 1) % nRows
		m.applyFocus()
		m.errMsg = ""

	case "shift+tab", "up":
		m.focusRow = (m.focusRow + nRows - 1) % nRows
		m.applyFocus()
		m.errMsg = ""

	case "ctrl+s":
		return m.trySave()

	case "enter":
		if m.focusRow >= nRows-1 {
			return m.trySave()
		}
		m.focusRow++
		m.applyFocus()

	default:
		if f := m.fields[m.focusRow]; f != nil {
			f.HandleKey(msg)
		}
	}
	return m, nil
}

func (m Model) trySave() (Model, tea.Cmd) {
	name := m.fields[fName].Value()
	path := m.fields[fPath].Value()
	if name == "" {
		m.errMsg = "Name is required"
		return m, nil
	}
	if path == "" {
		m.errMsg = "Path is required"
		return m, nil
	}
	m.errMsg = ""
	saved := MsgProjectSaved{Name: name, Path: path, OldSlug: m.oldSlug}
	return m, func() tea.Msg { return saved }
}
