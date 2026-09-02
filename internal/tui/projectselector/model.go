// Package projectselector implements the project switcher modal shown when the user presses [P].
package projectselector

import (
	"os"
	"strings"
	"unicode"

	"github.com/WariKoda/drift/internal/project"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgProjectChosen is emitted when the user confirms a project (Enter / 1-9).
type MsgProjectChosen struct {
	Project project.Project
}

// MsgSelectorCancelled is emitted when the user presses Esc / ctrl+c.
type MsgSelectorCancelled struct{}

// MsgOpenDashboard is emitted when the user presses m to manage projects.
type MsgOpenDashboard struct{}

// entry is a project plus whether its path is missing on disk.
type entry struct {
	proj    project.Project
	missing bool
}

// Model is the project selector modal.
type Model struct {
	projects    []entry
	filtered    []entry
	query       string
	cursor      int
	currentSlug string
	statusMsg   string
	Width       int
	Height      int
}

// New builds a picker from the given projects (already sorted by the caller;
// typically registry.Active()). currentSlug marks the active project in the list
// (may be empty). Missing directories are flagged via os.Stat.
func New(projects []project.Project, currentSlug string, width, height int) Model {
	entries := make([]entry, 0, len(projects))
	for _, p := range projects {
		missing := false
		if info, err := os.Stat(p.Path); err != nil || !info.IsDir() {
			missing = true
		}
		entries = append(entries, entry{proj: p, missing: missing})
	}
	m := Model{
		projects:    entries,
		currentSlug: currentSlug,
		Width:       width,
		Height:      height,
	}
	m.applyFilter()
	return m
}

// Init satisfies the sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
}

// SetStatus sets a one-line error shown under the picker footer.
func (m *Model) SetStatus(msg string) { m.statusMsg = msg }

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	m.statusMsg = ""

	switch msg.String() {
	case "esc", "ctrl+c":
		return m, func() tea.Msg { return MsgSelectorCancelled{} }

	case "m":
		if m.query == "" {
			return m, func() tea.Msg { return MsgOpenDashboard{} }
		}
		m.query += "m"
		m.applyFilter()

	case "enter":
		return m.choose(m.cursor)

	case "1", "2", "3", "4", "5", "6", "7", "8", "9":
		return m.choose(int(msg.String()[0] - '1'))

	case "down", "ctrl+n":
		if m.cursor < len(m.filtered)-1 {
			m.cursor++
		}

	case "up", "ctrl+p":
		if m.cursor > 0 {
			m.cursor--
		}

	case "backspace", "ctrl+h":
		if len(m.query) > 0 {
			r := []rune(m.query)
			m.query = string(r[:len(r)-1])
			m.applyFilter()
		}

	default:
		added := false
		for _, r := range msg.Runes {
			if unicode.IsPrint(r) {
				m.query += string(r)
				added = true
			}
		}
		if added {
			m.applyFilter()
		}
	}
	return m, nil
}

func (m Model) choose(idx int) (Model, tea.Cmd) {
	if idx < 0 || idx >= len(m.filtered) {
		return m, nil
	}
	m.cursor = idx
	e := m.filtered[idx]
	if e.missing {
		m.statusMsg = "Path not found: " + e.proj.Path
		return m, nil
	}
	p := e.proj
	return m, func() tea.Msg { return MsgProjectChosen{Project: p} }
}

func (m *Model) applyFilter() {
	q := strings.ToLower(m.query)
	m.filtered = nil
	for _, e := range m.projects {
		p := e.proj
		if q == "" ||
			strings.Contains(strings.ToLower(p.Name), q) ||
			strings.Contains(strings.ToLower(p.Slug), q) ||
			strings.Contains(strings.ToLower(p.Path), q) {
			m.filtered = append(m.filtered, e)
		}
	}
	if m.cursor >= len(m.filtered) {
		m.cursor = max(0, len(m.filtered)-1)
	}
}
