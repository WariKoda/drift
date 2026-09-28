package tui

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
)

func TestDeleteHostFailureShowsInHostManager(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	// Without a project slug DeleteProjectHost has no store to write to.
	cfg := &config.MergedConfig{ProjectRoot: t.TempDir()}
	app, err := New(cfg.ProjectRoot, cfg, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.state.Screen = ScreenHostManager
	app.state.TermWidth, app.state.TermHeight = 100, 30
	app.hostManager = hostmanager.New(cfg, 100, 30)

	model, _ := app.Update(hostmanager.MsgDeleteHost{Name: "prod", Scope: config.ScopeProject})
	app = model.(App)
	if app.state.Screen != ScreenHostManager {
		t.Fatalf("screen = %v, want host manager", app.state.Screen)
	}
	if view := app.hostManager.View(); !strings.Contains(view, "Delete failed") {
		t.Fatalf("host manager does not show delete failure:\n%s", view)
	}
}
