// Package dashboard implements the project dashboard — the optional landing
// screen that lists registered drift projects and lets the user open one.
package dashboard

import (
	"os"
	"strings"

	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/WariKoda/drift/internal/tui/mouse"
	"github.com/WariKoda/drift/internal/tui/textfield"
	tea "github.com/charmbracelet/bubbletea"
)

// entry is a single rendered row: a project plus whether its path is missing.
type entry struct {
	proj    project.Project
	missing bool
}

// Model is the dashboard screen.
type Model struct {
	reg        *project.Registry
	entries    []entry
	allEntries []entry
	cursor     int
	filter     textfield.TextField
	filterMode bool

	showArchived  bool
	confirmDelete bool
	statusMsg     string
	returnable    bool

	clicks mouse.ClickTracker

	Width  int
	Height int
}

// New builds a dashboard Model from the given registry.
func New(reg *project.Registry, width, height int) Model {
	m := Model{reg: reg, Width: width, Height: height, filter: textfield.TextField{Label: "Filter", MaxLen: 256}}
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

	m.allEntries = make([]entry, 0, len(projects))
	for _, p := range projects {
		missing := false
		if info, err := os.Stat(p.Path); err != nil || !info.IsDir() {
			missing = true
		}
		m.allEntries = append(m.allEntries, entry{proj: p, missing: missing})
	}
	m.filterEntries()
}

// filterEntries uses the last registry snapshot without reading project paths.
func (m *Model) filterEntries() {
	slug := ""
	if current := m.currentEntry(); current != nil {
		slug = current.proj.Slug
	}
	query := strings.ToLower(strings.TrimSpace(m.filter.Value()))
	m.entries = make([]entry, 0, len(m.allEntries))
	for _, e := range m.allEntries {
		if query == "" || strings.Contains(strings.ToLower(e.proj.Name), query) || strings.Contains(strings.ToLower(e.proj.Slug), query) || strings.Contains(strings.ToLower(e.proj.Path), query) {
			m.entries = append(m.entries, e)
		}
	}
	m.cursor = 0
	for i := range m.entries {
		if m.entries[i].proj.Slug == slug {
			m.cursor = i
			break
		}
	}
	m.clicks = mouse.ClickTracker{}
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
func (m *Model) SetStatus(msg string) { m.statusMsg = styles.SafeText(msg) }

// SetReturnable marks whether Esc should return to the previous screen
// instead of quitting drift. Landing-page dashboard stays false (Esc/q quit).
func (m *Model) SetReturnable(v bool) { m.returnable = v }

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
}
