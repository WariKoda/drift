// Package hostmanager implements the host list / CRUD screen.
package hostmanager

import (
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/loading"
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// entry is a flat list item — either a section header or a host row.
type entry struct {
	isHeader bool
	scope    config.HostScope
	host     config.Host // zero if isHeader
}

// Model is the host manager screen.
type Model struct {
	cfg     *config.MergedConfig
	entries []entry
	cursor  int // index into entries (headers are skipped)
	offset  int // first entry rendered in the list viewport

	// delete confirmation
	confirmDelete bool
	deleteTarget  entry
	confirmReset  bool

	// connection test
	testing     bool   // true while async test is in flight
	testTarget  string // host name being tested
	testID      uint64
	testTracker *loading.Tracker
	trust       *tlstrust.Manager

	// status line
	statusMsg string

	// mouse
	clicks mouse.ClickTracker

	Width  int
	Height int
}

// New creates a Model from the current merged config.
func New(cfg *config.MergedConfig, width, height int) Model {
	m := Model{cfg: cfg, Width: width, Height: height}
	m.rebuild()
	return m
}

// Init implements tea.Model (partial).
func (m Model) Init() tea.Cmd { return nil }

// SetTrustManager provides certificate trust for connection tests.
func (m *Model) SetTrustManager(trust *tlstrust.Manager) {
	m.trust = trust
}

// Testing reports whether a connection test is in flight.
func (m Model) Testing() (string, *loading.Tracker, bool) {
	return m.testTarget, m.testTracker, m.testing
}

// AcceptsTestResult reports whether id belongs to the active connection test.
func (m Model) AcceptsTestResult(id uint64) bool {
	return m.testing && id == m.testID
}

// CancelTest aborts an in-flight connection test and ignores its result.
func (m *Model) CancelTest() {
	if m.testTracker != nil {
		m.testTracker.Cancel()
	}
	m.testID++
	if !m.testing {
		return
	}
	m.testing = false
	m.statusMsg = "Cancelled"
}

// StartsNetworkOperation reports whether key would start a connection test.
func (m Model) StartsNetworkOperation(key tea.KeyMsg) bool {
	return key.String() == "t" && m.currentEntry() != nil && !m.testing
}

// rebuild flattens global + project hosts into the entries slice.
func (m *Model) rebuild() {
	m.entries = nil

	{ // always show section header
		m.entries = append(m.entries, entry{isHeader: true, scope: config.ScopeGlobal})
		for _, h := range config.SortedHostsByName(m.cfg.GlobalHosts) {
			m.entries = append(m.entries, entry{scope: config.ScopeGlobal, host: h})
		}
	}

	m.entries = append(m.entries, entry{isHeader: true, scope: config.ScopeProject})
	for _, h := range config.SortedHostsByName(m.cfg.ProjectHosts) {
		m.entries = append(m.entries, entry{scope: config.ScopeProject, host: h})
	}

	// clamp cursor to a host row
	m.clampCursor()
}

// clampCursorUp ensures cursor sits on a host row when navigating upward.
// Headers are skipped backward so the cursor moves into the section above.
func (m *Model) clampCursorUp() {
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
	// skip headers backward
	for m.cursor > 0 && m.entries[m.cursor].isHeader {
		m.cursor--
	}
	// if index 0 is also a header there is no host row above — fall forward
	if m.entries[m.cursor].isHeader {
		m.clampCursor()
	}
	m.ensureCursorVisible()
}

// clampCursor ensures cursor sits on a host row (not a header).
func (m *Model) clampCursor() {
	if len(m.entries) == 0 {
		m.cursor = 0
		return
	}
	// clamp to bounds
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= len(m.entries) {
		m.cursor = len(m.entries) - 1
	}
	// skip headers forward
	for m.cursor < len(m.entries) && m.entries[m.cursor].isHeader {
		m.cursor++
	}
	// if we ran off the end, skip headers backward
	if m.cursor >= len(m.entries) {
		m.cursor = len(m.entries) - 1
		for m.cursor >= 0 && m.entries[m.cursor].isHeader {
			m.cursor--
		}
	}
	// Guard: if all entries are headers, park at 0 (no host rows exist yet)
	if m.cursor < 0 {
		m.cursor = 0
	}
	m.ensureCursorVisible()
}

// ensureCursorVisible moves the viewport only when keyboard navigation takes
// the cursor outside it. Mouse clicks within the viewport therefore do not
// shift the rows beneath a possible second click.
func (m *Model) ensureCursorVisible() {
	if len(m.entries) == 0 {
		m.offset = 0
		return
	}
	m.offset = max(0, min(m.offset, len(m.entries)-1))
	used := 0
	for i := m.offset; i < len(m.entries); i++ {
		span := entryRowSpan(m.entries[i])
		if used+span > m.listHeight() {
			break
		}
		if i == m.cursor {
			return
		}
		used += span
	}

	start := m.cursor
	used = 0
	for start >= 0 {
		span := entryRowSpan(m.entries[start])
		if used+span > m.listHeight() {
			break
		}
		used += span
		start--
	}
	m.offset = start + 1
}

// currentEntry returns the entry under the cursor, or nil if none.
func (m Model) currentEntry() *entry {
	if m.cursor < 0 || m.cursor >= len(m.entries) {
		return nil
	}
	e := m.entries[m.cursor]
	if e.isHeader {
		return nil
	}
	return &m.entries[m.cursor]
}

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
	m.ensureCursorVisible()
}
