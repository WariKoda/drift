package tui

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tui/hostform"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
	tea "github.com/charmbracelet/bubbletea"
)

func key(r rune) tea.KeyMsg { return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}} }

// step feeds msg to app and then the message its command produces, if any.
func step(t *testing.T, app App, msg tea.Msg) App {
	t.Helper()
	model, cmd := app.Update(msg)
	app = model.(App)
	if cmd != nil {
		model, _ = app.Update(cmd())
		app = model.(App)
	}
	return app
}

func hostManagerApp(t *testing.T, cfg *config.MergedConfig) App {
	t.Helper()
	app, err := New(cfg.ProjectRoot, cfg, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.state.Screen = ScreenHostManager
	app.state.TermWidth, app.state.TermHeight = 140, 30
	app.hostManager = hostmanager.New(cfg, 140, 30)
	return app
}

func TestLinkHostOfAnotherProject(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	staging := config.Host{Name: "staging", Hostname: "staging.kunde-x.de", Port: 22, User: "web", RootPath: "/stage",
		Auth: config.Auth{Type: "password", Password: "secret"}}
	if err := config.SaveProjectHost(&config.MergedConfig{ProjectSlug: "shop-a"}, staging, ""); err != nil {
		t.Fatal(err)
	}
	cfg, err := config.Load(t.TempDir(), "shop-b")
	if err != nil {
		t.Fatal(err)
	}
	app := hostManagerApp(t, cfg)

	app = step(t, app, key('l'))
	if view := app.hostManager.View(); !strings.Contains(view, "PROJECT shop-a") || !strings.Contains(view, "staging.kunde-x.de") {
		t.Fatalf("picker does not list the other project's host:\n%s", view)
	}
	app = step(t, app, tea.KeyMsg{Type: tea.KeyEnter})
	if !strings.Contains(app.hostManager.View(), "becomes a global server") {
		t.Fatal("promotion was not confirmed first")
	}
	app = step(t, app, key('y'))
	if app.state.Screen != ScreenHostForm || !strings.Contains(app.hostForm.View(), "Link Server: staging") {
		t.Fatalf("link form not open:\n%s", app.hostForm.View())
	}
	app = step(t, app, tea.KeyMsg{Type: tea.KeyCtrlS})
	if app.state.Screen != ScreenHostManager {
		t.Fatalf("save did not return to the manager:\n%s", app.hostForm.View())
	}

	for _, slug := range []string{"shop-a", "shop-b"} {
		loaded, err := config.Load(t.TempDir(), slug)
		if err != nil {
			t.Fatal(err)
		}
		got := loaded.Hosts["staging"]
		if got.Server != "staging" || got.Auth.Password != "secret" || got.RootPath != "/stage" {
			t.Fatalf("%s host after linking = %+v", slug, got)
		}
	}
	if !strings.Contains(app.hostManager.View(), "→ staging") {
		t.Fatal("manager does not mark the link")
	}

	app = step(t, app, hostmanager.MsgDeleteHost{Name: "staging", Scope: config.ScopeGlobal})
	if view := app.hostManager.View(); !strings.Contains(view, "linked by shop-a, shop-b") {
		t.Fatalf("deleting a linked server does not say why it failed:\n%s", view)
	}
}

func TestSaveOffersLinkForSameEndpoint(t *testing.T) {
	for _, answer := range []rune{'l', 's'} {
		t.Run(string(answer), func(t *testing.T) {
			t.Setenv("XDG_CONFIG_HOME", t.TempDir())
			server := config.Host{Name: "kunde-x", Hostname: "kunde-x.de", Port: 22, User: "deploy", RootPath: "/var/www",
				Auth: config.Auth{Type: "agent"}}
			if err := config.SaveGlobalHost(&config.MergedConfig{}, server, ""); err != nil {
				t.Fatal(err)
			}
			cfg, err := config.Load(t.TempDir(), "shop")
			if err != nil {
				t.Fatal(err)
			}
			app := hostManagerApp(t, cfg)
			typed := server
			typed.Name = "prod"
			typed.RootPath = "/var/www/shop"
			app.hostForm = hostform.NewDuplicate(typed, config.ScopeProject, "shop", 140, 30)
			app.state.Screen = ScreenHostForm

			app = step(t, app, tea.KeyMsg{Type: tea.KeyCtrlS})
			if app.state.Screen != ScreenHostForm || !strings.Contains(app.hostForm.View(), `Same server as global server "kunde-x"`) {
				t.Fatalf("no link offer:\n%s", app.hostForm.View())
			}
			app = step(t, app, key(answer))
			if app.state.Screen != ScreenHostManager {
				t.Fatalf("answer %q did not save:\n%s", answer, app.hostForm.View())
			}

			loaded, err := config.Load(t.TempDir(), "shop")
			if err != nil {
				t.Fatal(err)
			}
			got := loaded.Hosts["prod"]
			wantServer := ""
			if answer == 'l' {
				wantServer = "kunde-x"
			}
			if got.Server != wantServer || got.RootPath != "/var/www/shop" || got.Hostname != "kunde-x.de" {
				t.Fatalf("saved host = %+v", got)
			}
		})
	}
}
