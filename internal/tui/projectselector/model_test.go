package projectselector

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
	case "up":
		return tea.KeyMsg{Type: tea.KeyUp}
	case "down":
		return tea.KeyMsg{Type: tea.KeyDown}
	case "ctrl+n":
		return tea.KeyMsg{Type: tea.KeyCtrlN}
	case "ctrl+p":
		return tea.KeyMsg{Type: tea.KeyCtrlP}
	}
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
}

func dispatch(m Model, k tea.KeyMsg) (Model, tea.Msg) {
	m, cmd := m.Update(k)
	if cmd == nil {
		return m, nil
	}
	return m, cmd()
}

func TestEnterEmitsProjectChosen(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}, "", 80, 24)
	_, out := dispatch(m, key("enter"))
	chosen, ok := out.(MsgProjectChosen)
	if !ok {
		t.Fatalf("expected MsgProjectChosen, got %T", out)
	}
	if chosen.Project.Slug != "a" {
		t.Fatalf("chosen slug = %q, want a", chosen.Project.Slug)
	}
}

func TestEnterOnMissingPathDoesNotEmit(t *testing.T) {
	m := New([]project.Project{
		{Slug: "gone", Name: "Gone", Path: "/no/such/path/drift-test"},
	}, "", 80, 24)
	m, out := dispatch(m, key("enter"))
	if out != nil {
		t.Fatalf("missing path should not emit chosen, got %T", out)
	}
	if m.statusMsg == "" {
		t.Fatal("expected a status message for missing path")
	}
}

func TestTypingFiltersList(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
		{Slug: "z", Name: "Zeta", Path: dir},
	}, "", 80, 24)
	m, _ = dispatch(m, key("z"))
	if len(m.filtered) != 1 {
		t.Fatalf("filtered count = %d, want 1", len(m.filtered))
	}
	if m.filtered[0].proj.Slug != "z" {
		t.Fatalf("filtered slug = %q, want z", m.filtered[0].proj.Slug)
	}
}

func TestMWithQueryFiltersInsteadOfManage(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "acme", Name: "Acme", Path: dir},
		{Slug: "zeta", Name: "Zeta", Path: dir},
	}, "", 80, 24)
	m, _ = dispatch(m, key("a"))
	m, out := dispatch(m, key("c"))
	if out != nil {
		t.Fatalf("typing should not emit, got %T", out)
	}
	m, out = dispatch(m, key("m"))
	if out != nil {
		t.Fatalf("m in a query should not open the dashboard, got %T", out)
	}
	if m.query != "acm" {
		t.Fatalf("query = %q, want acm", m.query)
	}
	if len(m.filtered) != 1 || m.filtered[0].proj.Slug != "acme" {
		t.Fatalf("filtered = %v, want acme", m.filtered)
	}
}

func TestMEmitsOpenDashboard(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}, "", 80, 24)
	_, out := dispatch(m, key("m"))
	if _, ok := out.(MsgOpenDashboard); !ok {
		t.Fatalf("expected MsgOpenDashboard, got %T", out)
	}
}

func TestEscEmitsSelectorCancelled(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}, "", 80, 24)
	_, out := dispatch(m, key("esc"))
	if _, ok := out.(MsgSelectorCancelled); !ok {
		t.Fatalf("expected MsgSelectorCancelled, got %T", out)
	}
}

func TestNumberKeyOpensFirstFiltered(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
		{Slug: "z", Name: "Zeta", Path: dir},
	}, "", 80, 24)
	m, _ = dispatch(m, key("z"))
	_, out := dispatch(m, key("1"))
	chosen, ok := out.(MsgProjectChosen)
	if !ok {
		t.Fatalf("expected MsgProjectChosen, got %T", out)
	}
	if chosen.Project.Slug != "z" {
		t.Fatalf("chosen slug = %q, want z (first filtered)", chosen.Project.Slug)
	}
}

func TestJKAreQueryCharacters(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "alpha", Name: "Alpha", Path: dir},
		{Slug: "kajak", Name: "Kajak", Path: dir},
	}, "", 80, 24)
	m, out := dispatch(m, key("j"))
	if out != nil {
		t.Fatalf("j should not emit, got %T", out)
	}
	if m.query != "j" {
		t.Fatalf("query = %q, want j", m.query)
	}
	if m.cursor != 0 {
		t.Fatalf("j must not move the cursor, got %d", m.cursor)
	}
	m, _ = dispatch(m, key("k"))
	if m.query != "jk" {
		t.Fatalf("query = %q, want jk", m.query)
	}
}

func TestArrowsMoveCursor(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
		{Slug: "z", Name: "Zeta", Path: dir},
	}, "", 80, 24)
	m, _ = dispatch(m, key("down"))
	if m.cursor != 1 {
		t.Fatalf("cursor after down = %d, want 1", m.cursor)
	}
	m, _ = dispatch(m, key("up"))
	if m.cursor != 0 {
		t.Fatalf("cursor after up = %d, want 0", m.cursor)
	}
	m, _ = dispatch(m, key("ctrl+n"))
	if m.cursor != 1 {
		t.Fatalf("cursor after ctrl+n = %d, want 1", m.cursor)
	}
	m, _ = dispatch(m, key("ctrl+p"))
	if m.cursor != 0 {
		t.Fatalf("cursor after ctrl+p = %d, want 0", m.cursor)
	}
}

func TestQDoesNotCancel(t *testing.T) {
	dir := t.TempDir()
	m := New([]project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}, "", 80, 24)
	m, out := dispatch(m, key("q"))
	if out != nil {
		t.Fatalf("q should type into the query, got %T", out)
	}
	if m.query != "q" {
		t.Fatalf("query = %q, want q", m.query)
	}
}
