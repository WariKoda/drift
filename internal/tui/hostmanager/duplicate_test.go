package hostmanager

import (
	"fmt"
	"reflect"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

func TestDuplicateHost(t *testing.T) {
	for _, scope := range []config.HostScope{config.ScopeGlobal, config.ScopeProject} {
		for _, tc := range []struct {
			name     string
			occupied []string
			want     string
		}{
			{"free", nil, "prod-copy"},
			{"suffix", []string{"prod-copy", "prod-copy-2"}, "prod-copy-3"},
			{"gap", []string{"prod-copy", "prod-copy-3"}, "prod-copy-2"},
		} {
			t.Run(fmt.Sprintf("%d/%s", scope, tc.name), func(t *testing.T) {
				original := config.Host{Name: "prod", Hostname: "example.com", Auth: config.Auth{Type: "password", Password: "secret"}, Mappings: []config.Mapping{{Local: "src", Remote: "app"}}}
				hosts := []config.Host{original}
				for _, name := range tc.occupied {
					hosts = append(hosts, config.Host{Name: name})
				}
				cfg := &config.MergedConfig{GlobalHosts: hosts, ProjectHosts: []config.Host{{Name: tc.want}}}
				if scope == config.ScopeProject {
					cfg.GlobalHosts, cfg.ProjectHosts = cfg.ProjectHosts, cfg.GlobalHosts
				}
				m := New(cfg, 120, 24)
				for i, e := range m.entries {
					if !e.isHeader && e.scope == scope && e.host.Name == "prod" {
						m.cursor = i
					}
				}
				_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'c'}})
				if cmd == nil {
					t.Fatal("copy returned no command")
				}
				msg, ok := cmd().(MsgOpenForm)
				if !ok || !msg.Duplicate || msg.OldName != "" || msg.Scope != scope || msg.Host == nil {
					t.Fatalf("unexpected message: %+v", msg)
				}
				want := original
				want.Name = tc.want
				if !reflect.DeepEqual(*msg.Host, want) {
					t.Fatal("copy lost host fields or chose wrong name")
				}
				if hosts[0].Name != "prod" {
					t.Fatal("copy renamed original")
				}
				if !strings.Contains(m.View(), "[c]copy") {
					t.Fatal("copy help missing")
				}
			})
		}
	}
}

func TestDuplicateWithoutSelection(t *testing.T) {
	m := New(&config.MergedConfig{}, 80, 24)
	for _, cursor := range []int{-1, 0, 1, 2} {
		m.cursor = cursor
		_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'c'}})
		if cmd != nil {
			t.Fatalf("cursor %d copied without a host", cursor)
		}
	}
}
