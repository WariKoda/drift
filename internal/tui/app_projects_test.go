package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/dashboard"
	"github.com/WariKoda/drift/internal/tui/diffview"
	"github.com/WariKoda/drift/internal/tui/projectselector"
	tea "github.com/charmbracelet/bubbletea"
)

func TestProjectPreparationRunsOnlyInCommand(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	oldDir, newDir := t.TempDir(), t.TempDir()
	a, err := New(oldDir, nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	p := project.Project{Slug: "new", Name: "New", Path: newDir}
	updated, prepare := a.Update(projectselector.MsgProjectChosen{Project: p})
	a = updated.(App)
	if prepare == nil || a.projectRequest == nil {
		t.Fatal("project switch did not schedule preparation")
	}
	if a.state.WorkingDir != oldDir {
		t.Fatal("Update switched the project before running its command")
	}
	// A config created after Update must still be read by the preparation.
	if err := os.MkdirAll(config.Dir(), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(config.Dir(), "config.toml"), []byte("invalid = ["), 0o600); err != nil {
		t.Fatal(err)
	}
	result := prepare().(msgProjectPrepared)
	if result.err == nil {
		t.Fatal("preparation used config loaded synchronously in Update")
	}
	updated, _ = a.Update(result)
	a = updated.(App)
	if a.state.WorkingDir != oldDir || a.projectRequest != nil {
		t.Fatal("failed preparation replaced the existing project")
	}
	if !strings.Contains(a.globalError, "Cannot open project") {
		t.Fatalf("failure was not surfaced: %q", a.globalError)
	}
}

func TestProjectPreparationFailureKeepsExistingResources(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	a, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	conn, host, _ := loopbackConnection(t)
	a.diffView = diffview.New(nil, host, conn, nil, 80, 24)
	a.state.SelectedHost = &host
	a.state.Screen = ScreenDashboard
	a.dashboard = dashboard.New(&project.Registry{}, 80, 24)
	pending := a.beginDiffRequest()
	updated, prepare := a.Update(dashboard.MsgProjectChosen{Project: project.Project{
		Slug: "missing", Path: filepath.Join(t.TempDir(), "missing"),
	}})
	a = updated.(App)
	updated, _ = a.Update(prepare())
	a = updated.(App)
	if a.state.Screen != ScreenDashboard || a.diffRequest != pending || a.state.SelectedHost == nil {
		t.Fatal("failed preparation detached the existing session")
	}
	if !strings.Contains(a.dashboard.View(), "Cannot open project") {
		t.Fatal("dashboard did not show the preparation error")
	}
	select {
	case <-conn.Done():
		t.Fatal("failed preparation closed the existing connection")
	default:
	}
	a.Close()
	select {
	case <-conn.Done():
	default:
		t.Fatal("existing connection was no longer reachable by Close")
	}
}

func TestCancelledProjectPreparationIsDiscarded(t *testing.T) {
	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyEsc},
		{Type: tea.KeyRunes, Runes: []rune{'q'}},
		{Type: tea.KeyCtrlC},
	} {
		t.Run(key.String(), func(t *testing.T) {
			t.Setenv("XDG_CONFIG_HOME", t.TempDir())
			oldDir := t.TempDir()
			a, err := New(oldDir, nil, nil, nil, ScreenBrowser, false)
			if err != nil {
				t.Fatal(err)
			}
			updated, prepare := a.Update(projectselector.MsgProjectChosen{Project: project.Project{
				Slug: "new", Path: t.TempDir(),
			}})
			a = updated.(App)
			updated, quit := a.Update(key)
			a = updated.(App)
			if quit != nil || a.projectRequest != nil {
				t.Fatal("cancel did not abandon preparation without quitting")
			}
			updated, _ = a.Update(prepare())
			a = updated.(App)
			if a.state.WorkingDir != oldDir {
				t.Fatal("late cancelled result switched projects")
			}
		})
	}
}

func TestSupersededProjectPreparationUsesLatestSize(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	a, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	first := project.Project{Slug: "first", Name: "First", Path: t.TempDir()}
	second := project.Project{Slug: "second", Name: "Second", Path: t.TempDir()}
	updated, prepareFirst := a.Update(projectselector.MsgProjectChosen{Project: first})
	a = updated.(App)
	updated, prepareSecond := a.Update(projectselector.MsgProjectChosen{Project: second})
	a = updated.(App)
	updated, _ = a.Update(prepareFirst())
	a = updated.(App)
	if a.state.WorkingDir == first.Path || a.projectRequest == nil {
		t.Fatal("superseded result consumed the latest request")
	}
	updated, _ = a.Update(tea.WindowSizeMsg{Width: 97, Height: 31})
	a = updated.(App)
	updated, _ = a.Update(prepareSecond())
	a = updated.(App)
	if a.state.WorkingDir != second.Path || a.browser.Width != 97 || a.browser.Height != 31 {
		t.Fatal("prepared project did not use the latest request and window size")
	}
}

func TestProjectOpenedTimestampIsPersistedInCommand(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	p := project.Project{Slug: "new", Name: "New", Path: t.TempDir()}
	reg := &project.Registry{Projects: []project.Project{p}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	a, err := New(t.TempDir(), nil, store, reg, ScreenDashboard, false)
	if err != nil {
		t.Fatal(err)
	}
	updated, prepare := a.Update(dashboard.MsgProjectChosen{Project: p})
	a = updated.(App)
	updated, record := a.Update(prepare())
	a = updated.(App)
	onDisk, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if !onDisk.Find(p.Slug).OpenedAt.IsZero() {
		t.Fatal("Update persisted the timestamp synchronously")
	}
	if record == nil {
		t.Fatal("switch did not schedule timestamp persistence")
	}
	// An independent process changes the project after the switch was prepared.
	onDisk.Find(p.Slug).Name = "Renamed"
	if err := store.Save(onDisk); err != nil {
		t.Fatal(err)
	}
	updated, _ = a.Update(record())
	a = updated.(App)
	onDisk, err = store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if onDisk.Find(p.Slug).OpenedAt.IsZero() || onDisk.Find(p.Slug).Name != "Renamed" {
		t.Fatal("timestamp command did not preserve the fresh project record")
	}
	if a.registry.Find(p.Slug).OpenedAt.IsZero() {
		t.Fatal("timestamp result did not refresh the in-memory registry")
	}
	if a.state.ActiveProject.Name != "Renamed" || !strings.Contains(a.browser.View(), "Renamed") {
		t.Fatal("timestamp result left stale project labels")
	}
}

func TestProjectOpenedTimestampFailureIsVisible(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	p := project.Project{Slug: "new", Name: "New", Path: t.TempDir()}
	reg := &project.Registry{Projects: []project.Project{p}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	a, err := New(t.TempDir(), nil, store, reg, ScreenDashboard, false)
	if err != nil {
		t.Fatal(err)
	}
	updated, prepare := a.Update(dashboard.MsgProjectChosen{Project: p})
	a = updated.(App)
	updated, record := a.Update(prepare())
	a = updated.(App)
	if err := os.WriteFile(store.Path(), []byte("invalid = ["), 0o600); err != nil {
		t.Fatal(err)
	}
	updated, _ = a.Update(record())
	a = updated.(App)
	if a.state.WorkingDir != p.Path || a.registry != reg {
		t.Fatal("timestamp failure undid the switch or replaced the registry")
	}
	if !strings.Contains(a.globalError, "record last-opened") {
		t.Fatalf("timestamp failure was swallowed: %q", a.globalError)
	}
}

func TestDelayedProjectOpenedResultRefreshesReloadedRegistry(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	p := project.Project{Slug: "new", Name: "New", Path: t.TempDir()}
	reg := &project.Registry{Projects: []project.Project{p}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	a, err := New(t.TempDir(), nil, store, reg, ScreenDashboard, false)
	if err != nil {
		t.Fatal(err)
	}
	updated, prepare := a.Update(dashboard.MsgProjectChosen{Project: p})
	a = updated.(App)
	updated, record := a.Update(prepare())
	a = updated.(App)
	updated, _ = a.Update(browser.MsgOpenDashboard{})
	a = updated.(App)
	updated, _ = a.Update(projectselector.MsgOpenDashboard{})
	a = updated.(App)
	if a.registry == reg {
		t.Fatal("picker did not reload the registry")
	}
	updated, reload := a.Update(record())
	a = updated.(App)
	if reload == nil {
		t.Fatal("delayed timestamp acknowledgement did not refresh the registry")
	}
	updated, _ = a.Update(reload())
	a = updated.(App)
	if a.registry.Find(p.Slug).OpenedAt.IsZero() {
		t.Fatal("delayed timestamp left the dashboard with a stale registry")
	}
	if err := a.persist(nil, func(reg *project.Registry) error {
		reg.Find(p.Slug).Archived = true
		return nil
	}); err != nil {
		t.Fatalf("timestamp acknowledgement left a spurious edit conflict: %v", err)
	}
}

func TestProjectOpenedTimestampKeepsSwitchOrder(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	first := project.Project{Slug: "first", Name: "First", Path: t.TempDir()}
	second := project.Project{Slug: "second", Name: "Second", Path: t.TempDir()}
	reg := &project.Registry{Projects: []project.Project{first, second}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	a, err := New(t.TempDir(), nil, store, reg, ScreenDashboard, false)
	if err != nil {
		t.Fatal(err)
	}
	updated, prepare := a.Update(dashboard.MsgProjectChosen{Project: first})
	a = updated.(App)
	updated, recordFirst := a.Update(prepare())
	a = updated.(App)
	updated, prepare = a.Update(projectselector.MsgProjectChosen{Project: second})
	a = updated.(App)
	updated, recordSecond := a.Update(prepare())
	a = updated.(App)
	// Execute in reverse order, independently of which project remains active.
	updated, _ = a.Update(recordSecond())
	a = updated.(App)
	updated, reload := a.Update(recordFirst())
	a = updated.(App)
	if reload != nil {
		updated, _ = a.Update(reload())
		a = updated.(App)
	}
	onDisk, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if !onDisk.Find(first.Slug).OpenedAt.Before(onDisk.Find(second.Slug).OpenedAt) {
		t.Fatal("background command order reversed last-opened ordering")
	}
	if a.state.ActiveProject.Slug != second.Slug {
		t.Fatal("late timestamp result changed the active project")
	}
}

func TestProjectOpenedTimestampPreservesNewerExternalOpen(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	p := project.Project{Slug: "new", Name: "New", Path: t.TempDir()}
	reg := &project.Registry{Projects: []project.Project{p}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	a, err := New(t.TempDir(), nil, store, reg, ScreenDashboard, false)
	if err != nil {
		t.Fatal(err)
	}
	updated, prepare := a.Update(dashboard.MsgProjectChosen{Project: p})
	a = updated.(App)
	updated, record := a.Update(prepare())
	a = updated.(App)
	fresh, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	newer := time.Now().UTC().Add(time.Hour)
	fresh.Find(p.Slug).OpenedAt = newer
	if err := store.Save(fresh); err != nil {
		t.Fatal(err)
	}
	updated, _ = a.Update(record())
	a = updated.(App)
	if !a.registry.Find(p.Slug).OpenedAt.Equal(newer) {
		t.Fatal("delayed command overwrote a newer last-opened timestamp")
	}
}
