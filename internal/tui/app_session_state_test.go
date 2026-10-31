package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/diffview"
	"github.com/WariKoda/drift/internal/tui/hostform"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
	"github.com/WariKoda/drift/internal/tui/projectform"
	"github.com/WariKoda/drift/internal/tui/projectselector"
)

func connectedSessionApp(t *testing.T) (*App, config.Host) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	root := t.TempDir()
	server := ftptest.Start(t, 1)
	server.AddFile("/old/file.txt", "remote")
	host := server.Host(t)
	host.RootPath = "/old"
	cfg, err := config.Load(root, "audit")
	if err != nil {
		t.Fatal(err)
	}
	if err := config.SaveProjectHost(cfg, host, ""); err != nil {
		t.Fatal(err)
	}
	a, err := New(root, cfg, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { a.Close() })
	host = cfg.Hosts[host.Name]
	load := a.browser.StartRemote(host)
	updated, _ := a.Update(load())
	a = updated.(App)
	if a.browser.Connection() == nil {
		t.Fatal("real FTP browser load failed")
	}
	updated, _ = a.Update(browser.MsgOpenHostManager{})
	a = updated.(App)
	return &a, host
}

func TestSavedActiveHostInvalidatesOldSession(t *testing.T) {
	a, host := connectedSessionApp(t)
	conn := a.browser.Connection()
	a.browser.RemoteSelection.Toggle("/old/file.txt")
	host.RootPath = "/new"
	updated, closeCmd := a.Update(hostform.MsgHostSaved{Host: host, Scope: config.ScopeProject, OldName: host.Name, IsEdit: true, KeepConnection: true})
	*a = updated.(App)
	if closeCmd == nil || a.browser.Connection() != nil || a.browser.RemoteSelection.Count() != 0 {
		t.Fatal("host edit retained the old connection or remote selections")
	}
	if _, assigned := a.browser.RemoteHost(); assigned {
		t.Fatal("refresh can still reconnect with the old host snapshot")
	}
	select {
	case <-conn.Done():
		t.Fatal("Update closed the connection synchronously")
	default:
	}
	closeCmd()
	select {
	case <-conn.Done():
	default:
		t.Fatal("returned command did not close the old connection")
	}
	if a.state.Config.Hosts[host.Name].RootPath != "/new" {
		t.Fatal("host edit was not saved")
	}
}

func TestDeletedActiveHostInvalidatesOldSession(t *testing.T) {
	a, host := connectedSessionApp(t)
	updated, closeCmd := a.Update(hostmanager.MsgDeleteHost{Name: host.Name, Scope: config.ScopeProject})
	*a = updated.(App)
	if closeCmd == nil || a.browser.Connection() != nil {
		t.Fatal("deleted host retained a connection")
	}
	closeCmd()
}

func TestUnrelatedHostSavePreservesCurrentSession(t *testing.T) {
	a, host := connectedSessionApp(t)
	conn := a.browser.Connection()
	host.Name = "unrelated"
	updated, closeCmd := a.Update(hostform.MsgHostSaved{Host: host, Scope: config.ScopeProject, KeepConnection: true})
	*a = updated.(App)
	if closeCmd != nil || a.browser.Connection() != conn {
		t.Fatal("saving an unrelated host detached the active session")
	}
}

func TestEditedActiveProjectReloadsRootAndName(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	oldPath, newPath := t.TempDir(), t.TempDir()
	p := project.Project{Slug: "audit", Name: "Audit", Path: oldPath}
	reg := &project.Registry{Projects: []project.Project{p}}
	store := project.NewStore()
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	cfg, err := config.Load(oldPath, p.Slug)
	if err != nil {
		t.Fatal(err)
	}
	a, err := New(oldPath, cfg, store, reg, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { a.Close() }()
	updated, prepare := a.Update(projectform.MsgProjectSaved{Name: "Moved", Path: newPath, OldSlug: p.Slug})
	a = updated.(App)
	if prepare == nil || a.state.WorkingDir != oldPath {
		t.Fatal("project save did not prepare its new root asynchronously")
	}
	updated, _ = a.Update(prepare())
	a = updated.(App)
	if a.state.WorkingDir != newPath || a.state.Config.ProjectRoot != newPath || !strings.Contains(a.browser.View(), "Moved") {
		t.Fatal("active project save did not replace its browser root and name")
	}
}

func TestSameSlugWithChangedPathIsNotSkipped(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	oldPath, newPath := t.TempDir(), t.TempDir()
	a, err := New(oldPath, nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { a.Close() }()
	a.state.ActiveProject = &project.Project{Slug: "audit", Path: oldPath}
	updated, cmd := a.Update(projectselector.MsgProjectChosen{Project: project.Project{Slug: "audit", Path: newPath}})
	a = updated.(App)
	if cmd == nil || a.projectRequest == nil {
		t.Fatal("same slug prevented opening the changed project path")
	}
}

func TestConfirmedDeleteUpdatesBrowserAndScopeSnapshots(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	root := t.TempDir()
	path := filepath.Join(root, "deleted.txt")
	if err := os.WriteFile(path, []byte("content"), 0o600); err != nil {
		t.Fatal(err)
	}
	cfg, err := config.Load(root, "")
	if err != nil {
		t.Fatal(err)
	}
	a, err := New(root, cfg, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { a.Close() }()
	a.browser.Selection.Toggle(path)
	a.browser.Selection.Toggle("/unrelated")
	a.browser.RemoteSelection.Toggle("/deleted.txt")
	updated, _ := a.Update(browser.MsgSyncRequested{Selection: a.browser.Selection.Clone(), RemoteSelection: a.browser.RemoteSelection.Clone()})
	a = updated.(App)
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	host := config.Host{Name: "audit"}
	a.state.Screen, a.state.SelectedHost = ScreenDiffView, &host
	updated, _ = a.Update(diffview.MsgScopeReloadRequested{DeletedLocal: []string{path}, DeletedRemote: []string{"/deleted.txt"}})
	a = updated.(App)
	if a.browser.Selection.IsMarked(path) || a.state.Selection.IsMarked(path) || a.browser.RemoteSelection.Count() != 0 || a.state.RemoteSelection.Count() != 0 {
		t.Fatal("confirmed delete remained selected in browser or scope snapshot")
	}
	if !a.browser.Selection.IsMarked("/unrelated") {
		t.Fatal("confirmed delete removed an unrelated mark")
	}
	a.state.SelectedHost = nil
	updated, _ = a.Update(diffview.MsgBackToBrowser{})
	a = updated.(App)
	if a.browser.Selection.Count() != 0 || a.state.Selection != a.browser.Selection || a.state.RemoteSelection != a.browser.RemoteSelection {
		t.Fatal("return from comparison did not restore live browser selection ownership")
	}
}
