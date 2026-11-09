package diffview

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestAutoLayoutUsesRemainingColumnsAndKeepsSource(t *testing.T) {
	m := New([]diff.Session{{Result: stateFixture()}}, config.Host{}, nil, nil, 190, 20)
	m.SetPreferences(config.DiffPreferences{Layout: "auto"})
	if m.splitLayout() {
		t.Fatal("auto used terminal width instead of diff width")
	}
	m.toggleAllFolds()
	m.remapScrollToSource(40)
	m.SetSize(220, 20)
	if !m.splitLayout() || m.topSourceLine() != 40 {
		t.Fatal("auto split lost source anchor")
	}
	m.SetSize(120, 20)
	if m.splitLayout() || m.topSourceLine() != 40 {
		t.Fatal("auto unified lost source anchor")
	}
}

func TestLayoutViewFitsNarrowTerminals(t *testing.T) {
	for _, layout := range []string{"unified", "split", "auto"} {
		for _, width := range []int{1, 10, 39, 80, 220} {
			m := New([]diff.Session{{LocalPath: "file", RemotePath: "/file", Result: stateFixture()}}, config.Host{}, nil, nil, width, 20)
			m.SetPreferences(config.DiffPreferences{Layout: layout})
			for _, help := range []bool{false, true} {
				m.showHelp = help
				for _, line := range strings.Split(m.View(), "\n") {
					if ansi.StringWidth(line) > width {
						t.Fatalf("layout=%s width=%d: %q", layout, width, line)
					}
				}
			}
		}
	}
}

func TestHorizontalScrollAndHelpDoNotChangeDecisions(t *testing.T) {
	result := &diff.DiffResult{Lines: []diff.DiffLine{{Text: strings.Repeat("a", 200) + "END", Kind: diff.LineAdded, RemoteNum: 1}}}
	m := New([]diff.Session{{Result: result}}, config.Host{}, nil, nil, 100, 20)
	cmd := m.Init()
	m, _ = m.Update(cmd())
	decision := m.syncDirs[0]
	for i := 0; i < 100; i++ {
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRight})
	}
	if columns, _ := m.columnLimit(); m.horizontal == 0 || m.horizontal > columns {
		t.Fatal("horizontal offset not bounded")
	}
	m, _ = m.handleKey(keyMsg("?"))
	offset := m.horizontal
	m, _ = m.handleKey(keyMsg("v"))
	m, _ = m.handleKey(tea.KeyMsg{Type: tea.KeyLeft})
	if !m.showHelp || m.horizontal != offset || m.preferences.Layout != "" || m.syncDirs[0] != decision {
		t.Fatal("help allowed hidden actions")
	}
	m, _ = m.handleKey(keyMsg("?"))
	m, _ = m.handleKey(keyMsg("v"))
	if !m.splitLayout() || m.syncDirs[0] != decision {
		t.Fatal("layout changed action")
	}
}
