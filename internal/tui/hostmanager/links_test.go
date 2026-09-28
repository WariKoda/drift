package hostmanager

import (
	"reflect"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

func runeKey(r rune) tea.KeyMsg { return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}} }

func TestNewInProjectSectionOffersLink(t *testing.T) {
	cfg := &config.MergedConfig{ProjectSlug: "shop", ProjectHosts: []config.Host{{Name: "prod"}}}
	for answer, want := range map[rune]tea.Msg{
		'n': MsgOpenForm{Scope: config.ScopeProject},
		'l': MsgLinkPickerRequested{},
	} {
		m := New(cfg, 120, 24)
		for i, e := range m.entries {
			if !e.isHeader && e.scope == config.ScopeProject {
				m.cursor = i
			}
		}
		m, cmd := m.Update(runeKey('n'))
		if cmd != nil || !m.choosingNew {
			t.Fatal("n in the project section did not ask what to create")
		}
		_, cmd = m.Update(runeKey(answer))
		if cmd == nil || !reflect.DeepEqual(cmd(), want) {
			t.Fatalf("answer %q did not produce %T", answer, want)
		}
	}

	// Without a project there is nothing to link to, so n opens the form.
	m := New(&config.MergedConfig{}, 120, 24)
	if _, cmd := m.Update(runeKey('n')); cmd == nil {
		t.Fatal("n without a project did not open the form")
	}
}

func TestPickerSkipsHeaders(t *testing.T) {
	m := New(&config.MergedConfig{ProjectSlug: "shop"}, 120, 24)
	m.OpenPicker([]config.LinkTarget{
		{Host: config.Host{Name: "kunde-x"}},
		{Project: "shop-a", Host: config.Host{Name: "staging"}},
	}, map[string]string{"shop-a": "Shop A"})
	m, _ = m.Update(runeKey('j'))
	if got := m.picker[m.pickCursor].target.Host.Name; got != "staging" {
		t.Fatalf("cursor on %q after moving past a header", got)
	}
	m, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd != nil || !m.confirmPromote {
		t.Fatal("a host of another project was chosen without confirmation")
	}
	_, cmd = m.Update(runeKey('y'))
	if chosen, ok := cmd().(MsgLinkTargetChosen); !ok || chosen.Target.Project != "shop-a" {
		t.Fatal("confirmation did not choose the target")
	}
}
