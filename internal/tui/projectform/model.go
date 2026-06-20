// Package projectform implements the create/edit project form screen.
package projectform

import (
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/textfield"
	tea "github.com/charmbracelet/bubbletea"
)

// Field indices.
const (
	fName = 0
	fPath = 1
	nRows = 2
)

// Model is the project create/edit form.
type Model struct {
	fields   [nRows]*textfield.TextField
	focusRow int

	isEdit  bool
	oldSlug string
	errMsg  string

	Width  int
	Height int
}

// New returns a form for a new project. defaultName and defaultPath pre-fill
// the fields (e.g. with the current directory); pass "" to leave them blank.
func New(defaultName, defaultPath string, width, height int) Model {
	m := Model{Width: width, Height: height}
	m.initFields()
	if defaultName != "" {
		m.fields[fName].SetValue(defaultName)
	}
	if defaultPath != "" {
		m.fields[fPath].SetValue(defaultPath)
	}
	m.fields[fName].Focused = true
	return m
}

// NewEdit returns a form pre-filled with an existing project's values.
func NewEdit(p project.Project, width, height int) Model {
	m := Model{isEdit: true, oldSlug: p.Slug, Width: width, Height: height}
	m.initFields()
	m.fields[fName].SetValue(p.Name)
	m.fields[fPath].SetValue(p.Path)
	m.fields[fName].Focused = true
	return m
}

// Init satisfies the bubbletea sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// SetErr sets a validation/save error message on the form.
func (m *Model) SetErr(msg string) { m.errMsg = msg }

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
	bw := w - 22
	if bw < 20 {
		bw = 20
	}
	for _, f := range m.fields {
		if f != nil {
			f.Width = bw
		}
	}
}

func (m *Model) initFields() {
	bw := m.Width - 22
	if bw < 20 {
		bw = 20
	}
	m.fields[fName] = &textfield.TextField{Label: "Name", Width: bw, Placeholder: "KUNDE A", MaxLen: 64}
	m.fields[fPath] = &textfield.TextField{Label: "Path", Width: bw, Placeholder: "~/work/kunde-a"}
}

func (m *Model) applyFocus() {
	for i, f := range m.fields {
		if f != nil {
			f.Focused = (i == m.focusRow)
		}
	}
}
