package diffview

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// wrapFixture ends in a long changed line so the last screen depends on
// wrapped heights.
func wrapFixture() *diff.DiffResult {
	result := stateFixture()
	lines := append([]diff.DiffLine(nil), result.Lines...)
	lines[60].Text = strings.Repeat("wrapped words ", 30) + "TAIL"
	lines = append(lines, diff.DiffLine{Text: strings.Repeat("last line ", 30) + "END", Kind: diff.LineAdded, RemoteNum: 101})
	return &diff.DiffResult{Lines: lines}
}

func TestWrapKeyKeepsGeometryAndDisablesHorizontalScroll(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: wrapFixture()}}, config.Host{}, nil, nil, 100, 16)
	if !m.wrapping() {
		t.Fatal("wrapping is not the default")
	}
	m.SetPreferences(config.DiffPreferences{Wrap: new(bool)})
	m, _ = m.Update(m.Init()())
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRight})
	if m.horizontal == 0 {
		t.Fatal("fixture must allow horizontal scrolling")
	}
	m, _ = m.handleKey(keyMsg("w"))
	if !m.wrapping() || m.horizontal != 0 {
		t.Fatal("w did not enable wrapping or kept the horizontal offset")
	}
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRight})
	if m.horizontal != 0 {
		t.Fatal("right arrow scrolled a wrapped view")
	}
	m, _ = m.handleKey(keyMsg("G"))
	view := m.View()
	plain := stripANSI(view)
	if !strings.Contains(plain, "END") {
		t.Fatalf("end of the last wrapped line is not reachable:\n%s", plain)
	}
	rows := strings.Split(view, "\n")
	if len(rows) != m.Height {
		t.Fatalf("%d rows, want %d", len(rows), m.Height)
	}
	for _, row := range rows {
		if lipgloss.Width(row) != m.Width {
			t.Fatalf("row width %d: %q", lipgloss.Width(row), stripANSI(row))
		}
	}
	at := diff.IndexOfSourceLine(m.displayRows(), 60)
	m.scroll = at
	if hit := m.hitTest(m.fileListWidth()+1, m.contentTop()+1); hit.index != at {
		t.Fatalf("continuation row maps to row %d, want %d", hit.index, at)
	}
	if hit := m.hitTest(m.fileListWidth()+1, m.contentTop()+m.rowHeight(at)); hit.index != at+1 {
		t.Fatalf("row after the wrapped line maps to %d, want %d", hit.index, at+1)
	}
}

func TestWrappedViewCountsVisibleRowsForFolds(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: wrapFixture()}}, config.Host{}, nil, nil, 100, 12)
	at := diff.IndexOfSourceLine(m.displayRows(), 60)
	m.scroll = at
	if m.rowsOnScreen() >= m.viewportHeight() {
		t.Fatalf("wrapped rows on screen %d, viewport %d", m.rowsOnScreen(), m.viewportHeight())
	}
	if row, ok := m.firstVisibleFold(); ok {
		if index := diff.IndexOfSourceLine(m.displayRows(), row.FoldStart); index >= m.scroll+m.rowsOnScreen() {
			t.Fatal("a fold below the screen counted as visible")
		}
	}
}

func TestWrappedContinuousViewFillsTheScreen(t *testing.T) {
	sessions := []diff.Session{
		{LocalPath: "/p/a", Result: wrapFixture()},
		{LocalPath: "/p/b", Result: wrapFixture()},
	}
	for _, layout := range []string{"unified", "split"} {
		m := continuousModel(t, 120, 20, sessions...)
		m.SetPreferences(config.DiffPreferences{Continuous: true, Layout: layout})
		for step := 0; step < 12; step++ {
			view := m.View()
			rows := strings.Split(view, "\n")
			if len(rows) != m.Height {
				t.Fatalf("%s: %d rows", layout, len(rows))
			}
			for _, row := range rows {
				if lipgloss.Width(row) != m.Width {
					t.Fatalf("%s: row width %d: %q", layout, lipgloss.Width(row), stripANSI(row))
				}
			}
			m, _ = m.handleKey(keyMsg("pgdown"))
		}
		if !strings.Contains(stripANSI(m.View()), "END") {
			t.Fatalf("%s: the last wrapped line is not reachable", layout)
		}
	}
}
