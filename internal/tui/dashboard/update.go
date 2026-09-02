package dashboard

import (
	"github.com/WariKoda/drift/internal/project"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgProjectChosen is emitted when the user opens a project (Enter).
type MsgProjectChosen struct {
	Project project.Project
}

// MsgOpenProjectForm is emitted to create (Project == nil) or edit a project.
type MsgOpenProjectForm struct {
	Project *project.Project
}

// MsgDeleteProject is emitted when a delete is confirmed.
type MsgDeleteProject struct {
	Slug string
}

// MsgArchiveProject toggles a project's archived flag.
type MsgArchiveProject struct {
	Slug string
}

// MsgDashboardQuit is emitted when the user leaves the dashboard (quits drift).
type MsgDashboardQuit struct{}

// MsgDashboardBack is emitted when the user presses Esc on a returnable dashboard.
type MsgDashboardBack struct{}

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
	case tea.MouseMsg:
		return m.updateMouse(msg)
	case tea.KeyMsg:
		if m.confirmDelete {
			return m.updateConfirm(msg)
		}
		return m.updateNormal(msg)
	}
	return m, nil
}

// chooseCurrent opens the project under the cursor, or sets a status message
// when its path is missing. Enter, number keys, and a double click all go here.
func (m Model) chooseCurrent() (Model, tea.Cmd) {
	e := m.currentEntry()
	if e == nil {
		return m, nil
	}
	if e.missing {
		m.statusMsg = "Path not found: " + e.proj.Path + " — press [e] to fix"
		return m, nil
	}
	p := e.proj
	return m, func() tea.Msg { return MsgProjectChosen{Project: p} }
}

func (m Model) updateNormal(msg tea.KeyMsg) (Model, tea.Cmd) {
	m.statusMsg = ""

	switch msg.String() {
	case "j", "down":
		m.cursor++
		m.clampCursor()

	case "k", "up":
		m.cursor--
		m.clampCursor()

	case "g":
		m.cursor = 0
		m.clampCursor()

	case "G":
		m.cursor = len(m.entries) - 1
		m.clampCursor()

	case "enter":
		return m.chooseCurrent()

	case "n":
		return m, func() tea.Msg { return MsgOpenProjectForm{} }

	case "e":
		e := m.currentEntry()
		if e == nil {
			break
		}
		p := e.proj
		return m, func() tea.Msg { return MsgOpenProjectForm{Project: &p} }

	case "d", "delete":
		if m.currentEntry() != nil {
			m.confirmDelete = true
		}

	case "a":
		e := m.currentEntry()
		if e == nil {
			break
		}
		slug := e.proj.Slug
		return m, func() tea.Msg { return MsgArchiveProject{Slug: slug} }

	case ".":
		m.showArchived = !m.showArchived
		m.rebuild()

	case "1", "2", "3", "4", "5", "6", "7", "8", "9":
		idx := int(msg.String()[0] - '1')
		if idx < len(m.entries) {
			m.cursor = idx
			return m.chooseCurrent()
		}

	case "q":
		return m, func() tea.Msg { return MsgDashboardQuit{} }

	case "esc":
		if m.returnable {
			return m, func() tea.Msg { return MsgDashboardBack{} }
		}
		return m, func() tea.Msg { return MsgDashboardQuit{} }
	}

	return m, nil
}

func (m Model) updateConfirm(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case "y", "enter":
		e := m.currentEntry()
		m.confirmDelete = false
		if e == nil {
			break
		}
		slug := e.proj.Slug
		return m, func() tea.Msg { return MsgDeleteProject{Slug: slug} }
	default: // any other key cancels
		m.confirmDelete = false
	}
	return m, nil
}
