package dashboard

import (
	"testing"

	"github.com/WariKoda/drift/internal/project"
	tea "github.com/charmbracelet/bubbletea"
)

func key(s string) tea.KeyMsg {
	if len(s) == 1 {
		return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
	}
	switch s {
	case "enter":
		return tea.KeyMsg{Type: tea.KeyEnter}
	case "esc":
		return tea.KeyMsg{Type: tea.KeyEsc}
	}
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
}

func newModel(t *testing.T) Model {
	t.Helper()
	// Use the temp dir itself as an existing path so projects aren't flagged missing.
	dir := t.TempDir()
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
		{Slug: "z", Name: "Zeta", Path: dir},
	}}
	return New(reg, 80, 24)
}

func dispatch(m Model, k tea.KeyMsg) (Model, tea.Msg) {
	m, cmd := m.Update(k)
	if cmd == nil {
		return m, nil
	}
	return m, cmd()
}

func TestNavigationClamps(t *testing.T) {
	m := newModel(t)
	if m.cursor != 0 {
		t.Fatalf("initial cursor = %d, want 0", m.cursor)
	}
	m, _ = dispatch(m, key("k")) // up at top stays
	if m.cursor != 0 {
		t.Fatalf("cursor after up-at-top = %d, want 0", m.cursor)
	}
	m, _ = dispatch(m, key("G")) // jump to bottom
	if m.cursor != 1 {
		t.Fatalf("cursor after G = %d, want 1", m.cursor)
	}
	m, _ = dispatch(m, key("j")) // down at bottom stays
	if m.cursor != 1 {
		t.Fatalf("cursor after down-at-bottom = %d, want 1", m.cursor)
	}
}

func TestEnterEmitsProjectChosen(t *testing.T) {
	m := newModel(t)
	_, out := dispatch(m, key("enter"))
	chosen, ok := out.(MsgProjectChosen)
	if !ok {
		t.Fatalf("expected MsgProjectChosen, got %T", out)
	}
	if chosen.Project.Slug != "a" {
		t.Fatalf("chosen slug = %q, want a", chosen.Project.Slug)
	}
}

func TestDeleteRequiresConfirmation(t *testing.T) {
	m := newModel(t)
	m, out := dispatch(m, key("d"))
	if out != nil {
		t.Fatalf("d alone should not emit a message, got %T", out)
	}
	if !m.confirmDelete {
		t.Fatal("expected confirmDelete to be set")
	}
	m, out = dispatch(m, key("y"))
	del, ok := out.(MsgDeleteProject)
	if !ok {
		t.Fatalf("expected MsgDeleteProject, got %T", out)
	}
	if del.Slug != "a" {
		t.Fatalf("delete slug = %q, want a", del.Slug)
	}
}

func TestEnterOnMissingPathDoesNotEmit(t *testing.T) {
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "gone", Name: "Gone", Path: "/no/such/path/drift-test"},
	}}
	m := New(reg, 80, 24)
	m, out := dispatch(m, key("enter"))
	if out != nil {
		t.Fatalf("missing path should not emit chosen, got %T", out)
	}
	if m.statusMsg == "" {
		t.Fatal("expected a status message for missing path")
	}
}

func TestQuitEmitsDashboardQuit(t *testing.T) {
	m := newModel(t)
	_, out := dispatch(m, key("q"))
	if _, ok := out.(MsgDashboardQuit); !ok {
		t.Fatalf("expected MsgDashboardQuit, got %T", out)
	}
}
