package tui

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/dashboard"
	"github.com/WariKoda/drift/internal/tui/diffview"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
	"github.com/WariKoda/drift/internal/tui/loading"
	"github.com/WariKoda/drift/internal/tui/projectselector"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func makeProjectDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, ".drift"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".drift", "config.toml"), []byte("# project\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	return dir
}

func TestRegisterCandidate(t *testing.T) {
	emptyReg := &project.Registry{}

	// A repository is offered: registering is what gives it hosts.
	repo := t.TempDir()
	if err := os.MkdirAll(filepath.Join(repo, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	sub := filepath.Join(repo, "src")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	cfg := &config.MergedConfig{ProjectRoot: sub}
	if root, ok := registerCandidate(sub, cfg, emptyReg); !ok || root != repo {
		t.Fatalf("registerCandidate inside a repository = (%q, %v), want (%q, true)", root, ok, repo)
	}

	registered := &project.Registry{Projects: []project.Project{{Slug: "x", Path: repo}}}
	if _, ok := registerCandidate(sub, cfg, registered); ok {
		t.Fatal("a registered repository was offered again")
	}

	// Not a repository: nothing to offer.
	plain := t.TempDir()
	if _, ok := registerCandidate(plain, &config.MergedConfig{ProjectRoot: plain}, emptyReg); ok {
		t.Fatal("a plain directory was offered for registration")
	}

	// A directory that already belongs to a project is never offered.
	inProject := &config.MergedConfig{ProjectRoot: repo, ProjectSlug: "shop"}
	if _, ok := registerCandidate(sub, inProject, emptyReg); ok {
		t.Fatal("a directory inside an open project was offered")
	}

	if _, ok := registerCandidate(sub, nil, emptyReg); ok {
		t.Fatal("a nil config was offered")
	}
	if _, ok := registerCandidate(sub, cfg, nil); ok {
		t.Fatal("a nil registry was offered")
	}
}

func TestActiveNetworkOperationBlocksSecondOperation(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.startNetworkActivity(activityHostTest, "Testing host…", nil)

	model, cmd := app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("@")})
	if cmd != nil {
		t.Fatal("a second network operation was not blocked")
	}

	app = model.(App)
	if !app.loader.Active() {
		t.Fatal("blocked key finished the network activity")
	}
}

func TestNetworkActivityCancelStopsLoader(t *testing.T) {
	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyRunes, Runes: []rune("q")},
		{Type: tea.KeyCtrlC},
	} {
		t.Run(key.String(), func(t *testing.T) {
			app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser, false)
			if err != nil {
				t.Fatal(err)
			}
			tracker := loading.NewTracker("Testing host…")
			app.startNetworkActivity(activityHostTest, "Testing host…", tracker)

			model, cmd := app.Update(key)
			if cmd != nil {
				t.Fatal("cancel returned a command")
			}
			next := model.(App)
			if next.loader.Active() {
				t.Fatal("cancel left the loader running")
			}
			if !tracker.Canceled() {
				t.Fatal("cancel did not abort the tracker")
			}
		})
	}
}

func TestCtrlCQuitsWhenIdle(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}

	_, cmd := app.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	if cmd == nil {
		t.Fatal("Ctrl+C did not quit when idle")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatal("Ctrl+C command did not return tea.QuitMsg")
	}
}

func TestBackgroundErrorIsShownInCurrentView(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.startNetworkActivity(activityHostTest, "Testing host…", nil)

	model, _ := app.Update(hostmanager.MsgTestResult{HostName: "staging", Err: errors.New("connection refused")})
	next := model.(App)
	if next.loader.Active() {
		t.Fatal("connection test result did not finish the global activity")
	}
	if !strings.Contains(next.globalError, "connection refused") {
		t.Fatalf("global error = %q, want connection failure", next.globalError)
	}
}

func TestReplaceStatusLineUsesCurrentViewFooter(t *testing.T) {
	view := strings.Join([]string{"header", "content", "old status"}, "\n")
	got := replaceStatusLine(view, 30, 3, "Error: failed", true)
	lines := strings.Split(ansi.Strip(got), "\n")
	if len(lines) != 3 {
		t.Fatalf("status view has %d lines, want 3", len(lines))
	}
	if !strings.Contains(lines[2], "Error: failed") {
		t.Fatalf("footer = %q, want global error", lines[2])
	}
}

// countingConn is a remote.Client that records whether it was closed. The app
// must close the connection of a diff result it discards, and there is no way
// to observe that on a real connection without a server.
type countingConn struct{ closes int }

func (c *countingConn) Stat(string) (os.FileInfo, error)           { return nil, errors.New("unused") }
func (c *countingConn) ReadDir(string) ([]*fs.FileEntry, error)    { return nil, errors.New("unused") }
func (c *countingConn) Open(string) (io.ReadCloser, error)         { return nil, errors.New("unused") }
func (c *countingConn) ReadFile(string) ([]byte, error)            { return nil, errors.New("unused") }
func (c *countingConn) Upload(string, io.Reader) error             { return errors.New("unused") }
func (c *countingConn) WalkFiles(string, func(string) error) error { return errors.New("unused") }
func (c *countingConn) DeleteFile(string) error                    { return errors.New("unused") }
func (c *countingConn) Close() error                               { c.closes++; return nil }

// loadingApp returns an app in projectDir with a diff request in flight against
// host, mirroring what pressing [s] does.
func loadingApp(t *testing.T, projectDir string, reg *project.Registry, host config.Host) App {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	cfg, err := config.Load(projectDir, "shop")
	if err != nil {
		t.Fatal(err)
	}
	app, err := New(projectDir, cfg, nil, reg, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.state.TermWidth, app.state.TermHeight = 120, 40

	model, _ := app.Update(browser.MsgSyncRequested{
		Selection:       app.state.Selection,
		RemoteSelection: app.state.RemoteSelection,
		Host:            &host,
	})
	app = model.(App)
	if app.diffRequest == 0 {
		t.Fatal("sync request did not start a diff request")
	}
	return app
}

func TestDiffResultOfLeftProjectIsDiscarded(t *testing.T) {
	dirA, dirB := makeProjectDir(t), makeProjectDir(t)
	reg := &project.Registry{Projects: []project.Project{{Slug: "b", Name: "B", Path: dirB}}}

	app := loadingApp(t, dirA, reg, config.Host{Name: "hostA", Hostname: "a.example"})
	pending := app.diffRequest

	// The user opens another project while the diff load is still running.
	model, _ := app.Update(dashboard.MsgProjectChosen{Project: reg.Projects[0]})
	app = model.(App)
	if app.state.WorkingDir != dirB {
		t.Fatalf("working dir = %q, want %q", app.state.WorkingDir, dirB)
	}
	if app.diffRequest != 0 {
		t.Fatal("opening a project left the diff request pending")
	}
	if app.loader.Active() {
		t.Fatal("opening a project left the diff loader running")
	}

	// The result for the project that was left arrives late.
	conn := &countingConn{}
	model, _ = app.Update(diffview.MsgDiffLoaded{
		RequestID: pending,
		Host:      config.Host{Name: "hostA", Hostname: "a.example"},
		Sessions:  []diff.Session{{LocalPath: filepath.Join(dirA, "x.txt"), RemotePath: "/srv/a/x.txt"}},
		Conn:      conn,
	})
	app = model.(App)

	if app.state.Screen == ScreenDiffView {
		t.Error("a diff result from the project that was left opened the diff view")
	}
	if conn.closes != 1 {
		t.Errorf("connection closed %d times, want 1", conn.closes)
	}
	if app.state.SelectedHost != nil {
		t.Errorf("selected host = %v, want none", app.state.SelectedHost)
	}
}

func TestSupersededDiffResultIsDiscarded(t *testing.T) {
	dir := makeProjectDir(t)
	app := loadingApp(t, dir, nil, config.Host{Name: "hostA", Hostname: "a.example"})
	stale := app.diffRequest

	// A second request supersedes the first.
	current := app.beginDiffRequest()
	if current == stale {
		t.Fatal("second diff request reused the first ID")
	}

	conn := &countingConn{}
	model, _ := app.Update(diffview.MsgDiffLoaded{
		RequestID: stale,
		Host:      config.Host{Name: "hostA"},
		Conn:      conn,
	})
	app = model.(App)
	if app.state.Screen == ScreenDiffView {
		t.Error("superseded diff result opened the diff view")
	}
	if conn.closes != 1 {
		t.Errorf("connection closed %d times, want 1", conn.closes)
	}
	if app.diffRequest != current {
		t.Errorf("pending request = %d, want %d", app.diffRequest, current)
	}

	// The error message of a superseded request must not surface either.
	model, _ = app.Update(diffview.MsgDiffError{RequestID: stale, Err: errors.New("connect failed")})
	app = model.(App)
	if app.globalError != "" {
		t.Errorf("global error = %q, want none", app.globalError)
	}
}

func TestAcceptedDiffResultUsesHostFromResult(t *testing.T) {
	dir := makeProjectDir(t)
	app := loadingApp(t, dir, nil, config.Host{Name: "hostA", Hostname: "a.example"})

	host := config.Host{Name: "hostA", Hostname: "a.example", RootPath: "/srv/a"}
	model, _ := app.Update(diffview.MsgDiffLoaded{
		RequestID: app.diffRequest,
		Host:      host,
		Sessions:  []diff.Session{{LocalPath: filepath.Join(dir, "x.txt"), RemotePath: "/srv/a/x.txt"}},
	})
	app = model.(App)

	if app.state.Screen != ScreenDiffView {
		t.Fatalf("screen = %v, want ScreenDiffView", app.state.Screen)
	}
	if app.diffRequest != 0 {
		t.Error("accepted result left the diff request pending")
	}
	if app.state.SelectedHost == nil || app.state.SelectedHost.RootPath != host.RootPath {
		t.Errorf("selected host = %v, want %v", app.state.SelectedHost, host)
	}
}

// storedApp is loadingApp with a persisted registry, so the project picker can open.
func storedApp(t *testing.T, projectDir string, reg *project.Registry, host config.Host) App {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	cfg, err := config.Load(projectDir, "shop")
	if err != nil {
		t.Fatal(err)
	}
	app, err := New(projectDir, cfg, store, reg, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.state.TermWidth, app.state.TermHeight = 120, 40

	model, _ := app.Update(browser.MsgSyncRequested{
		Selection:       app.state.Selection,
		RemoteSelection: app.state.RemoteSelection,
		Host:            &host,
	})
	app = model.(App)
	if app.diffRequest == 0 {
		t.Fatal("sync request did not start a diff request")
	}
	return app
}

func TestProjectPickerKeepsPendingDiff(t *testing.T) {
	dir := makeProjectDir(t)
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}}
	app := storedApp(t, dir, reg, config.Host{Name: "hostA", Hostname: "a.example"})
	pending := app.diffRequest

	model, _ := app.Update(browser.MsgOpenDashboard{})
	app = model.(App)
	if app.state.Screen != ScreenProjectSelector {
		t.Fatalf("screen = %v, want ScreenProjectSelector", app.state.Screen)
	}
	if app.diffRequest != pending {
		t.Fatal("opening the picker abandoned the diff request")
	}
	if view := ansi.Strip(app.View()); !strings.Contains(view, "Switch project") {
		t.Fatal("picker overlay missing title")
	}

	model, _ = app.Update(projectselector.MsgSelectorCancelled{})
	app = model.(App)
	if app.state.Screen != ScreenBrowser {
		t.Fatalf("after cancel screen = %v, want ScreenBrowser", app.state.Screen)
	}
	if app.diffRequest != pending {
		t.Fatal("cancelling the picker abandoned the diff request")
	}
}

func TestProjectPickerSameProjectDoesNotReroot(t *testing.T) {
	dir := makeProjectDir(t)
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}}
	app := storedApp(t, dir, reg, config.Host{Name: "hostA", Hostname: "a.example"})
	pending := app.diffRequest
	if app.state.ActiveProject == nil {
		t.Fatal("expected ActiveProject to be bound from the registry")
	}

	model, _ := app.Update(projectselector.MsgProjectChosen{Project: *app.state.ActiveProject})
	app = model.(App)
	if app.state.Screen != ScreenBrowser {
		t.Fatalf("screen = %v, want ScreenBrowser", app.state.Screen)
	}
	if app.diffRequest != pending {
		t.Fatal("re-selecting the current project abandoned the diff request")
	}
	if app.state.WorkingDir != dir {
		t.Fatalf("working dir = %q, want %q", app.state.WorkingDir, dir)
	}
}

func TestDashboardBackReturnsToBrowser(t *testing.T) {
	dir := makeProjectDir(t)
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "a", Name: "Alpha", Path: dir},
	}}
	app := storedApp(t, dir, reg, config.Host{Name: "hostA", Hostname: "a.example"})
	pending := app.diffRequest

	model, _ := app.Update(browser.MsgOpenDashboard{})
	app = model.(App)
	model, _ = app.Update(projectselector.MsgOpenDashboard{})
	app = model.(App)
	if app.state.Screen != ScreenDashboard {
		t.Fatalf("screen = %v, want ScreenDashboard", app.state.Screen)
	}
	if app.diffRequest != pending {
		t.Fatal("opening the dashboard from the picker abandoned the diff request")
	}

	model, _ = app.Update(dashboard.MsgDashboardBack{})
	app = model.(App)
	if app.state.Screen != ScreenBrowser {
		t.Fatalf("after back screen = %v, want ScreenBrowser", app.state.Screen)
	}
	if app.diffRequest != pending {
		t.Fatal("Esc from the dashboard abandoned the diff request")
	}
}

func TestBrowserHeaderShowsProjectName(t *testing.T) {
	dir := makeProjectDir(t)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "kunde-a", Name: "KUNDE A", Path: dir},
	}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	cfg, err := config.Load(dir, "shop")
	if err != nil {
		t.Fatal(err)
	}
	app, err := New(dir, cfg, store, reg, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	app.state.TermWidth, app.state.TermHeight = 80, 24
	app.browser.SetSize(80, 24)

	view := ansi.Strip(app.View())
	if !strings.Contains(view, "KUNDE A") {
		t.Fatalf("header = %q, want project name", view)
	}
}
