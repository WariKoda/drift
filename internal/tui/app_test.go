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

func TestShouldPromptRegister(t *testing.T) {
	dir := makeProjectDir(t)

	cfg := &config.MergedConfig{ProjectRoot: dir}
	emptyReg := &project.Registry{}

	if !shouldPromptRegister(dir, cfg, emptyReg) {
		t.Fatal("unregistered project should prompt")
	}

	registered := &project.Registry{Projects: []project.Project{{Slug: "x", Path: dir}}}
	if shouldPromptRegister(dir, cfg, registered) {
		t.Fatal("registered project should not prompt")
	}

	// A directory without .drift must never prompt.
	plain := t.TempDir()
	if shouldPromptRegister(plain, &config.MergedConfig{ProjectRoot: plain}, emptyReg) {
		t.Fatal("non-project dir should not prompt")
	}

	// Nil registry/config guards.
	if shouldPromptRegister(dir, nil, emptyReg) || shouldPromptRegister(dir, cfg, nil) {
		t.Fatal("nil cfg/reg should not prompt")
	}
}

func TestActiveNetworkOperationBlocksQuitAndSecondOperation(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser)
	if err != nil {
		t.Fatal(err)
	}
	app.startNetworkActivity(activityHostTest, "Testing host…", nil)

	model, cmd := app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("q")})
	if cmd != nil {
		t.Fatal("q was not blocked during a network operation")
	}
	app = model.(App)
	model, cmd = app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("@")})
	if cmd != nil {
		t.Fatal("a second network operation was not blocked")
	}

	app = model.(App)
	_, cmd = app.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	if cmd == nil {
		t.Fatal("Ctrl+C did not quit during a network operation")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatal("Ctrl+C command did not return tea.QuitMsg")
	}
}

func TestBackgroundErrorIsShownInCurrentView(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser)
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

	cfg, err := config.Load(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	app, err := New(projectDir, cfg, nil, reg, ScreenBrowser)
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
	cfg, err := config.Load(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	app, err := New(projectDir, cfg, store, reg, ScreenBrowser)
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
	cfg, err := config.Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	app, err := New(dir, cfg, store, reg, ScreenBrowser)
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

func TestMigrationNotice(t *testing.T) {
	safe := migrationNotice(1, config.ExposureSafe)
	if !strings.Contains(safe, "Moved 1 credential") || !strings.Contains(safe, "access.toml") {
		t.Fatalf("notice does not say what moved where: %q", safe)
	}
	if strings.Contains(safe, "rotate") {
		t.Fatalf("an unreachable config asked for a rotation: %q", safe)
	}
	if got := migrationNotice(2, config.ExposureSafe); !strings.Contains(got, "2 credentials") {
		t.Fatalf("notice does not pluralise: %q", got)
	}
	if got := migrationNotice(1, config.ExposureTracked); !strings.Contains(got, "rotate") ||
		!strings.Contains(got, "git rm --cached") {
		t.Fatalf("a tracked config produced no rotation advice: %q", got)
	}
	if got := migrationNotice(1, config.ExposureUntracked); !strings.Contains(got, "committed") {
		t.Fatalf("an unignored config produced no history hint: %q", got)
	}
}

func TestMigrationRunsOnlyForCredentialsFoundInTheConfigFile(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser)
	if err != nil {
		t.Fatal(err)
	}
	if cmd := app.migrateProjectSecrets(); cmd != nil {
		t.Fatal("a nil config scheduled a migration")
	}

	// Credentials filled in from the secret store are already out of the
	// project, so they leave ProjectSecretsInFile empty.
	app.state.Config = &config.MergedConfig{
		ProjectHosts: []config.Host{{Name: "prod", Auth: config.Auth{Type: "password", Password: "hunter2"}}},
	}
	if cmd := app.migrateProjectSecrets(); cmd != nil {
		t.Fatal("a credential from the store scheduled a migration")
	}

	app.state.Config.ProjectSecretsInFile = []string{"prod"}
	if cmd := app.migrateProjectSecrets(); cmd == nil {
		t.Fatal("a credential found in the config file did not schedule a migration")
	}
}

func TestMigrationNoticeSurvivesKeysUntilEsc(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser)
	if err != nil {
		t.Fatal(err)
	}
	app.state.Config = &config.MergedConfig{ProjectSecretsInFile: []string{"prod"}}
	app.state.TermWidth, app.state.TermHeight = 160, 24

	model, _ := app.Update(msgSecretsMigrated{Count: 1, Exposure: config.ExposureTracked})
	app = model.(App)
	if app.secretWarning == "" {
		t.Fatal("a completed migration produced no notice")
	}
	if len(app.state.Config.ProjectSecretsInFile) != 0 {
		t.Fatal("the migration result did not clear ProjectSecretsInFile")
	}
	if !strings.Contains(ansi.Strip(app.View()), "rotate") {
		t.Fatal("the notice is not rendered in the status line")
	}

	model, _ = app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("j")})
	app = model.(App)
	if app.secretWarning == "" {
		t.Fatal("an ordinary key dismissed the notice")
	}

	model, _ = app.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if model.(App).secretWarning != "" {
		t.Fatal("esc did not dismiss the notice")
	}
}

func TestMigrationErrorIsSurfaced(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser)
	if err != nil {
		t.Fatal(err)
	}
	model, _ := app.Update(msgSecretsMigrated{Err: errors.New("disk full")})
	if got := model.(App).globalError; !strings.Contains(got, "disk full") {
		t.Fatalf("global error = %q, want the migration failure", got)
	}
}

func TestMigrationRunsForA016StoreEvenWithACleanProjectConfig(t *testing.T) {
	app, err := New(t.TempDir(), nil, nil, nil, ScreenBrowser)
	if err != nil {
		t.Fatal(err)
	}

	// The unit tests for the migration called it directly, so this gate was
	// never covered: a 0.1.6-alpha store plus a project config with nothing
	// left in it scheduled no migration, and the credentials in that store
	// became unreachable.
	app.state.Config = &config.MergedConfig{ProjectRoot: "/work/shop"}
	if cmd := app.migrateProjectSecrets(); cmd != nil {
		t.Fatal("a config with nothing to migrate scheduled a migration")
	}

	app.state.Config.LegacySecretStore = true
	if cmd := app.migrateProjectSecrets(); cmd == nil {
		t.Fatal("a 0.1.6-alpha secret store did not schedule a migration")
	}
}
