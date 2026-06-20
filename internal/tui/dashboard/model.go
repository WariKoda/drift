// Package dashboard implements the project dashboard — the optional landing
// screen that lists registered drift projects and lets the user open one.
package dashboard

import (
	"os"

	"github.com/WariKoda/drift/internal/project"
	tea "github.com/charmbracelet/bubbletea"
)

// entry is a single rendered row: a project plus whether its path is missing.
type entry struct {
	proj    project.Project
	missing bool
}

// Model is the dashboard screen.
type Model struct {
	reg     *project.Registry
	entries []entry
	cursor  int

	showArchived  bool
	confirmDelete bool
	statusMsg     string

	Width  int
	Height int
}

// New builds a dashboard Model from the given registry.
func New(reg *project.Registry, width, height int) Model {
	m := Model{reg: reg, Width: width, Height: height}
	m.rebuild()
	return m
}

// Init satisfies the bubbletea sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// Refresh re-reads the registry into the entry list (after an external change).
func (m *Model) Refresh(reg *project.Registry) {
	m.reg = reg
	m.rebuild()
}

// rebuild flattens the registry into entries, honouring the archived filter,
// and stats each path so missing directories can be flagged.
func (m *Model) rebuild() {
	var projects []project.Project
	if m.showArchived {
		projects = m.reg.All()
	} else {
		projects = m.reg.Active()
	}

	m.entries = make([]entry, 0, len(projects))
	for _, p := range projects {
		missing := false
		if info, err := os.Stat(p.Path); err != nil || !info.IsDir() {
			missing = true
		}
		m.entries = append(m.entries, entry{proj: p, missing: missing})
	}
	m.clampCursor()
}

func (m *Model) clampCursor() {
	if len(m.entries) == 0 {
		m.cursor = 0
		return
	}
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= len(m.entries) {
		m.cursor = len(m.entries) - 1
	}
}

// currentEntry returns the entry under the cursor, or nil if the list is empty.
func (m Model) currentEntry() *entry {
	if m.cursor < 0 || m.cursor >= len(m.entries) {
		return nil
	}
	return &m.entries[m.cursor]
}

// SetStatus sets a one-line status/error message shown in the footer.
func (m *Model) SetStatus(msg string) { m.statusMsg = msg }

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
}
