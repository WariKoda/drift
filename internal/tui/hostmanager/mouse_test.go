package hostmanager

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/charmbracelet/x/ansi"
)

// testHostModel builds a host manager sized 80x24 holding one global and one
// project section, each with the given number of hosts. Entry rows start at
// y=headerLines, and every section contributes one non-selectable header row.
func testHostModel(globalHosts, projectHosts int) Model {
	m := Model{Width: 80, Height: 24, cfg: &config.MergedConfig{}}

	m.entries = append(m.entries, entry{isHeader: true, scope: config.ScopeGlobal})
	for i := 0; i < globalHosts; i++ {
		m.entries = append(m.entries, entry{
			scope: config.ScopeGlobal,
			host:  config.Host{Name: "global" + string(rune('0'+i)), Hostname: "example.com"},
		})
	}

	m.entries = append(m.entries, entry{isHeader: true, scope: config.ScopeProject})
	for i := 0; i < projectHosts; i++ {
		m.entries = append(m.entries, entry{
			scope: config.ScopeProject,
			host:  config.Host{Name: "project" + string(rune('0'+i)), Hostname: "example.org"},
		})
	}

	return m
}

func TestHostManagerHitTest(t *testing.T) {
	// Entry layout with 2 global and 1 project host. A section header spans
	// two rows — a blank spacer above its label — so rows and entry indexes
	// drift apart as sections accumulate:
	//
	//   row 0  (blank spacer)   entry 0, GLOBAL header
	//   row 1  GLOBAL HOSTS     entry 0
	//   row 2  global0          entry 1
	//   row 3  global1          entry 2
	//   row 4  (blank spacer)   entry 3, PROJECT header
	//   row 5  PROJECT HOSTS    entry 3
	//   row 6  project0         entry 4
	tests := []struct {
		name string
		y    int
		want int
	}{
		{"the title row is not selectable", 0, noHit},
		{"the separator row is not selectable", 1, noHit},
		{"the spacer above the global header is not selectable", headerLines, noHit},
		{"the global section header is not selectable", headerLines + 1, noHit},
		{"the first global host", headerLines + 2, 1},
		{"the second global host", headerLines + 3, 2},
		{"the spacer above the project header is not selectable", headerLines + 4, noHit},
		{"the project section header is not selectable", headerLines + 5, noHit},
		{"the first project host", headerLines + 6, 4},
		{"a blank row below the last entry", headerLines + 7, noHit},
		{"the status bar is not selectable", 23, noHit},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			m := testHostModel(2, 1)
			// x must not matter: rows span the full width.
			for _, x := range []int{0, 20, 70} {
				if got := m.hitTest(x, tc.y); got != tc.want {
					t.Errorf("hitTest(%d,%d) = %d, want %d", x, tc.y, got, tc.want)
				}
			}
		})
	}
}

func TestHostManagerHitTestBelowViewport(t *testing.T) {
	// A terminal short enough that the list shows only the two header rows and
	// one host: anything below belongs to the footer, even though more entries
	// exist.
	m := testHostModel(5, 0)
	m.Height = headerLines + 3 + footerLines

	if got := m.hitTest(5, headerLines+2); got != 1 {
		t.Errorf("hitTest inside the viewport = %d, want 1", got)
	}
	if got := m.hitTest(5, headerLines+3); got != noHit {
		t.Errorf("hitTest below the viewport = %d, want noHit", got)
	}
}

// TestViewFitsTerminalHeight guards the row budget. Section headers used to
// emit a blank line the budget never counted, so the view ran two lines longer
// than the terminal and pushed the status bar off screen.
func TestViewFitsTerminalHeight(t *testing.T) {
	for _, tc := range []struct{ global, project int }{
		{0, 0}, {2, 1}, {5, 5}, {30, 30},
	} {
		m := testHostModel(tc.global, tc.project)
		lines := strings.Split(m.View(), "\n")
		if len(lines) != m.Height {
			t.Errorf("%d global + %d project hosts rendered %d lines, want %d",
				tc.global, tc.project, len(lines), m.Height)
		}
	}
}

// TestHostManagerLayoutConstantsMatchView binds the constants to the rendered
// view, so a layout change fails here instead of shifting every click.
func TestHostManagerLayoutConstantsMatchView(t *testing.T) {
	m := testHostModel(2, 1)

	lines := strings.Split(m.View(), "\n")
	if len(lines) < m.Height {
		t.Fatalf("view rendered %d lines, want at least %d", len(lines), m.Height)
	}

	t.Run("headerLines is the section header's spacer row", func(t *testing.T) {
		row := ansi.Strip(lines[headerLines])
		if strings.TrimSpace(row) != "" {
			t.Errorf("row %d = %q, want the blank spacer", headerLines, row)
		}
	})

	t.Run("the section label follows its spacer", func(t *testing.T) {
		row := ansi.Strip(lines[headerLines+1])
		if !strings.Contains(row, "GLOBAL") {
			t.Errorf("row %d = %q, want the global section header", headerLines+1, row)
		}
	})

	t.Run("the first host follows its section header", func(t *testing.T) {
		row := ansi.Strip(lines[headerLines+2])
		if !strings.Contains(row, "global0") {
			t.Errorf("row %d = %q, want the first global host", headerLines+2, row)
		}
	})

	t.Run("the list ends before the footer", func(t *testing.T) {
		lastRow := headerLines + m.listHeight() - 1
		if lastRow+footerLines != m.Height-1 {
			t.Errorf("last list row %d + %d footer lines = %d, want %d",
				lastRow, footerLines, lastRow+footerLines, m.Height-1)
		}
	})
}
