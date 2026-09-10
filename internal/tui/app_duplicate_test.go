package tui

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tui/hostform"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
	tea "github.com/charmbracelet/bubbletea"
)

func TestDuplicateHostRouting(t *testing.T) {
	for _, scope := range []config.HostScope{config.ScopeGlobal, config.ScopeProject} {
		for _, cancel := range []bool{false, true} {
			t.Run(fmt.Sprintf("%d/cancel=%v", scope, cancel), func(t *testing.T) {
				t.Setenv("XDG_CONFIG_HOME", t.TempDir())
				cfg := &config.MergedConfig{ProjectSlug: "shop", ProjectRoot: t.TempDir()}
				interval := 75
				original := config.Host{Name: "prod", Hostname: "example.com", Port: 2222, Protocol: "sftp", User: "deploy", RootPath: "/srv", Auth: config.Auth{Type: "keyfile", KeyFile: "~/.ssh/deploy", Passphrase: "phrase"}, KeepAliveInterval: &interval, Mappings: []config.Mapping{{Local: "src", Remote: "app"}}}
				save := config.SaveGlobalHost
				path := filepath.Join(config.Dir(), "config.toml")
				if scope == config.ScopeProject {
					save = config.SaveProjectHost
					path = filepath.Join(config.Dir(), "projects", "shop.toml")
				}
				if err := save(cfg, original, ""); err != nil {
					t.Fatal(err)
				}
				before, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				memoryBefore, err := json.Marshal(cfg)
				if err != nil {
					t.Fatal(err)
				}
				app, err := New(cfg.ProjectRoot, cfg, nil, nil, ScreenBrowser, false)
				if err != nil {
					t.Fatal(err)
				}
				app.state.Screen = ScreenHostManager
				app.state.TermWidth, app.state.TermHeight = 100, 30
				app.hostManager = hostmanager.New(cfg, 100, 30)
				model, cmd := app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'c'}})
				if cmd == nil {
					t.Fatal("copy did not open form")
				}
				app = model.(App)
				model, _ = app.Update(cmd())
				app = model.(App)
				if app.state.Screen != ScreenHostForm || !strings.Contains(app.hostForm.View(), "Duplicate Host") {
					t.Fatal("duplicate routed to wrong form")
				}
				afterOpen, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				memoryOpen, err := json.Marshal(cfg)
				if err != nil {
					t.Fatal(err)
				}
				if !bytes.Equal(before, afterOpen) || !bytes.Equal(memoryBefore, memoryOpen) {
					t.Fatal("opening duplicate changed config")
				}
				key := tea.KeyCtrlS
				if cancel {
					key = tea.KeyEsc
				}
				model, cmd = app.Update(tea.KeyMsg{Type: key})
				app = model.(App)
				if cmd == nil {
					t.Fatal("form returned no command")
				}
				msg := cmd()
				if !cancel {
					saved, ok := msg.(hostform.MsgHostSaved)
					want := original
					want.Name = "prod-copy"
					if !ok || saved.Scope != scope || saved.OldName != "" || !reflect.DeepEqual(saved.Host, want) {
						t.Fatal("form did not emit an independent create")
					}
				}
				model, _ = app.Update(msg)
				app = model.(App)
				if app.state.Screen != ScreenHostManager {
					t.Fatal("did not return to manager")
				}
				if cancel {
					after, err := os.ReadFile(path)
					if err != nil {
						t.Fatal(err)
					}
					memoryAfter, err := json.Marshal(cfg)
					if err != nil {
						t.Fatal(err)
					}
					if !bytes.Equal(before, after) || !bytes.Equal(memoryBefore, memoryAfter) {
						t.Fatal("cancel changed config")
					}
					return
				}
				loaded, err := config.Load(cfg.ProjectRoot, cfg.ProjectSlug)
				if err != nil {
					t.Fatal(err)
				}
				hosts := loaded.GlobalHosts
				if scope == config.ScopeProject {
					hosts = loaded.ProjectHosts
				}
				if len(hosts) != 2 || !reflect.DeepEqual(hosts[0], original) {
					t.Fatal("save replaced or changed original")
				}
				want := original
				want.Name = "prod-copy"
				if !reflect.DeepEqual(hosts[1], want) {
					t.Fatal("saved copy lost fields")
				}
				// A second copy uses the refreshed manager list rather than the old names.
				model, cmd = app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'c'}})
				if cmd == nil {
					t.Fatal("second copy returned no command")
				}
				opened := cmd().(hostmanager.MsgOpenForm)
				if opened.Host.Name != "prod-copy-2" {
					t.Fatalf("second copy name = %q", opened.Host.Name)
				}
				app = model.(App)
				model, _ = app.Update(opened)
				app = model.(App)
				// A conflicting save stays in the form and exposes the config error.
				model, _ = app.Update(hostform.MsgHostSaved{Host: original, Scope: scope})
				app = model.(App)
				if app.state.Screen != ScreenHostForm || !strings.Contains(app.hostForm.View(), "already exists") {
					t.Fatal("collision did not stay in form with error")
				}
			})
		}
	}
}
