// Package browser implements the yazi-like local file browser TUI component.
package browser

import (
	"os"
	"path/filepath"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/nibra180/drift-tui/internal/fs"
)

// Model is the bubbletea sub-model for the file browser screen.
type Model struct {
	// tree state
	WorkDir  string
	entries  []*fs.FileEntry // flat visible list
	cursor   int
	offset   int            // scroll offset into entries

	// selection
	Selection *fs.SelectionState

	// visual selection mode
	visualMode  bool
	visualStart int

	// filter
	filterMode bool
	filter     string

	// help overlay
	showHelp bool

	// terminal dimensions (set by root app on WindowSizeMsg)
	Width  int
	Height int

	// status message (transient)
	statusMsg string
}

// New creates a browser Model for the given directory.
// Initial width/height will be overwritten by the first WindowSizeMsg.
func New(workDir string) (Model, error) {
	entries, err := fs.ReadDir(workDir)
	if err != nil {
		return Model{}, err
	}
	for _, e := range entries {
		e.Depth = 0
	}
	return Model{
		WorkDir:   workDir,
		entries:   entries,
		Selection: fs.NewSelectionState(),
		Width:     80,
		Height:    24,
	}, nil
}

// Init satisfies the tea.Model interface (root app calls this).
func (m Model) Init() tea.Cmd {
	return nil
}

// SetSize updates the terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
	m.clampScroll()
}

// SetStatus sets a transient status message (e.g. error from a previous screen).
func (m *Model) SetStatus(msg string) {
	m.statusMsg = msg
}

// viewportHeight returns the number of lines available for entries.
func (m Model) viewportHeight() int {
	h := m.Height - 4 // header + 2 separators + status bar
	if h < 1 {
		return 1
	}
	return h
}

// clampScroll ensures cursor and offset are within bounds.
func (m *Model) clampScroll() {
	if len(m.entries) == 0 {
		m.cursor = 0
		m.offset = 0
		return
	}
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= len(m.entries) {
		m.cursor = len(m.entries) - 1
	}
	vh := m.viewportHeight()
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.cursor >= m.offset+vh {
		m.offset = m.cursor - vh + 1
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

// reload refreshes the top-level directory listing, preserving expanded state.
func (m *Model) reload() error {
	// Collect currently expanded paths
	expanded := map[string]bool{}
	for _, e := range m.entries {
		if e.Kind == fs.EntryDir && e.Expanded {
			expanded[e.Path] = true
		}
	}

	entries, err := fs.ReadDir(m.WorkDir)
	if err != nil {
		return err
	}
	for _, e := range entries {
		e.Depth = 0
	}
	m.entries = entries

	// Re-expand previously expanded dirs (best effort)
	for i := 0; i < len(m.entries); i++ {
		if expanded[m.entries[i].Path] {
			_ = m.expandAt(i)
		}
	}

	m.clampScroll()
	return nil
}

// absWorkDir returns the absolute display path of the working directory.
func (m Model) absWorkDir() string {
	abs, err := filepath.Abs(m.WorkDir)
	if err != nil {
		return m.WorkDir
	}
	home, _ := os.UserHomeDir()
	if home != "" && strings.HasPrefix(abs, home) {
		return "~" + abs[len(home):]
	}
	return abs
}
