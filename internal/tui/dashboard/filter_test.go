package dashboard

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

func TestFilterOpensAndDeletesTheVisibleProject(t *testing.T) {
	m := newModel(t)
	m, _ = dispatch(m, key("/"))
	m, out := dispatch(m, key("ZETA"))
	if out != nil || len(m.entries) != 1 || m.currentEntry().proj.Slug != "z" {
		t.Fatalf("filter selected the wrong project: %+v, %T", m.entries, out)
	}
	m, out = dispatch(m, key("enter"))
	if out != nil || m.filterMode {
		t.Fatal("finishing input opened the project")
	}
	_, out = dispatch(m, key("1"))
	if chosen, ok := out.(MsgProjectChosen); !ok || chosen.Project.Slug != "z" {
		t.Fatalf("number key opened the wrong filtered project: %#v", out)
	}
	m, _ = dispatch(m, key("d"))
	m, out = dispatch(m, key("y"))
	if deleted, ok := out.(MsgDeleteProject); !ok || deleted.Slug != "z" {
		t.Fatalf("confirmation deleted the wrong filtered project: %#v", out)
	}
}

func TestFilterConsumesActionsAndClearsBeforeLeaving(t *testing.T) {
	m := newModel(t)
	m.SetReturnable(true)
	m, _ = dispatch(m, key("/"))
	for _, value := range []string{"q", "n", "d", "a", "1"} {
		var out tea.Msg
		m, out = dispatch(m, key(value))
		if out != nil || m.confirmDelete {
			t.Fatalf("filter key %q ran a project action", value)
		}
	}
	if !strings.Contains(ansi.Strip(m.View()), "No matching projects") {
		t.Fatal("empty filtered list looks like an empty registry")
	}
	m.filter.SetValue("e\u0301")
	m, _ = dispatch(m, tea.KeyMsg{Type: tea.KeyBackspace})
	if m.filter.Value() != "" || len(m.entries) != 2 {
		t.Fatal("backspace did not remove the whole grapheme")
	}
	m, _ = dispatch(m, key("Zeta"))
	m, _ = dispatch(m, key("enter"))
	m, out := dispatch(m, key("esc"))
	if out != nil || m.filter.Value() != "" || len(m.entries) != 2 {
		t.Fatal("Escape left the dashboard before clearing the filter")
	}
	_, out = dispatch(m, key("esc"))
	if _, ok := out.(MsgDashboardBack); !ok {
		t.Fatalf("Escape after clearing did not return: %T", out)
	}
}

func TestCompactAndFilteredLayoutsKeepMouseTargets(t *testing.T) {
	m := newModel(t)
	m, _ = dispatch(m, key("/"))
	m, _ = dispatch(m, key("Zeta"))
	for _, height := range []int{8, 12, 19, 20, 24} {
		for _, width := range []int{20, 40, 80, 120} {
			m.SetSize(width, height)
			rows := strings.Split(m.View(), "\n")
			if len(rows) != height {
				t.Fatalf("%dx%d rendered %d rows", width, height, len(rows))
			}
			for _, row := range rows {
				if lipgloss.Width(row) > width {
					t.Fatalf("%dx%d row overflow: %q", width, height, row)
				}
			}
			if !strings.Contains(ansi.Strip(rows[m.listTop()]), "Zeta") || m.hitTest(m.blockLeft()+2, m.listTop()) != 0 || m.hitTest(m.blockLeft()+2, m.listTop()-1) != noHit {
				t.Fatalf("%dx%d filter and project hit regions disagree", width, height)
			}
			if height < 20 && m.listTop() != 3 {
				t.Fatalf("compact header still occupies %d rows", m.listTop())
			}
		}
	}
}
