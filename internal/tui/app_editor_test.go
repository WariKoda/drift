package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/certtrust"
	"github.com/WariKoda/drift/internal/tui/remoteeditor"
	tea "github.com/charmbracelet/bubbletea"
)

func editorApp(t *testing.T, tls bool) (*App, *ftptest.Server) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	t.Setenv("DRIFT_GUI_EDITOR_HELPER", "1")
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "file.txt"), []byte("local unchanged"), 0600); err != nil {
		t.Fatal(err)
	}
	start := ftptest.Start
	if tls {
		start = ftptest.StartTLS
	}
	server := start(t, 8)
	server.AddFile("/file.txt", "remote original")
	host := server.Host(t)
	terminal := false
	cfg := &config.MergedConfig{ProjectRoot: root, Editor: &config.Editor{
		Command: []string{os.Args[0], "-test.run=TestRemoteEditorGUIHelper", "--", "{file}"}, Terminal: &terminal,
	}}
	a, err := New(root, cfg, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	a.browser.SetSize(120, 24)
	a.state.TermWidth, a.state.TermHeight = 120, 24
	load := a.browser.StartRemote(host)
	updated, _ := a.Update(load())
	a = updated.(App)
	if tls {
		if a.certPrompt == nil {
			t.Fatal("initial FTPS trust prompt missing")
		}
		updated, retry := a.Update(certtrust.MsgDecision{Decision: certtrust.TrustSession, Challenge: a.certPrompt.Challenge})
		a = updated.(App)
		updated, _ = a.Update(runEditorCommand(t, retry))
		a = updated.(App)
	}
	if a.browser.Connection() == nil {
		t.Fatal("browser connection was not established")
	}
	a.browser, _ = a.browser.Update(tea.KeyMsg{Type: tea.KeyTab})
	a.browser.Selection.Toggle(filepath.Join(root, "file.txt"))
	a.browser.RemoteSelection.Toggle("/file.txt")
	app := &a
	t.Cleanup(func() { app.Close() })
	return app, server
}

func runEditorCommand(t *testing.T, cmd tea.Cmd) tea.Msg {
	t.Helper()
	if cmd == nil {
		t.Fatal("expected an editor command")
	}
	msg := cmd()
	if commands, ok := msg.(tea.BatchMsg); ok {
		// The first command does the work; the others are loader timers or watches.
		return runEditorCommand(t, commands[0])
	}
	return msg
}

func applyEditorMessage(t *testing.T, app *App, msg tea.Msg) tea.Cmd {
	t.Helper()
	updated, cmd := app.Update(msg)
	*app = updated.(App)
	return cmd
}

func startEditorForTest(t *testing.T, a *App) (browser.MsgEditRequested, string, tea.Cmd) {
	t.Helper()
	cmd := applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
	request := runEditorCommand(t, cmd).(browser.MsgEditRequested)
	cmd = applyEditorMessage(t, a, request)
	prepared := runEditorCommand(t, cmd).(remoteeditor.MsgResult)
	if prepared.Result.Err != nil {
		t.Fatal(prepared.Result.Err)
	}
	cmd = applyEditorMessage(t, a, prepared)
	return request, prepared.Result.File, cmd
}

func TestExternalEditorRoundTripRequiresConsentAndKeepsBrowserConnection(t *testing.T) {
	a, server := editorApp(t, false)
	conn := a.browser.Connection()
	request, file, cmd := startEditorForTest(t, a)
	if a.state.Screen != ScreenRemoteEditor || a.browser.Connection() != nil || a.remoteEditor.Connection() != conn {
		t.Fatal("connection ownership did not move to editor")
	}
	// Duplicated entry requests and covered scene actions must not close or replace it.
	if cmd := applyEditorMessage(t, a, request); cmd != nil {
		t.Fatal("duplicate request changed the active session")
	}
	applyEditorMessage(t, a, browser.MsgOpenDashboard{})
	if a.state.Screen != ScreenRemoteEditor {
		t.Fatal("covered scene replaced editor")
	}
	processDone := runEditorCommand(t, cmd)
	cmd = applyEditorMessage(t, a, processDone)
	captured := runEditorCommand(t, cmd)
	cmd = applyEditorMessage(t, a, captured)
	if cmd != nil || server.CommandCount("STOR") != 0 || a.loader.Active() {
		t.Fatal("editor completion uploaded without consent or left loader active")
	}
	if !strings.Contains(a.View(), "[u]") {
		t.Fatal("upload decision is not visible")
	}
	cmd = applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("u")})
	result := runEditorCommand(t, cmd).(remoteeditor.MsgResult)
	if !result.Result.Saved || result.Result.Err != nil {
		t.Fatalf("save: %+v", result.Result)
	}
	cmd = applyEditorMessage(t, a, result)
	closed := runEditorCommand(t, cmd).(remoteeditor.MsgClosed)
	applyEditorMessage(t, a, closed)
	if a.state.Screen != ScreenBrowser || a.browser.Connection() != conn || a.remoteEditor.Connection() != nil {
		t.Fatal("successful return lost the browser connection")
	}
	if content, _ := server.File("/file.txt"); content != "edited by helper" {
		t.Fatalf("remote = %q", content)
	}
	if data, err := os.ReadFile(filepath.Join(a.state.WorkingDir, "file.txt")); err != nil || string(data) != "local unchanged" {
		t.Fatal("remote edit modified the local project")
	}
	if a.browser.Selection.Count() != 1 || a.browser.RemoteSelection.Count() != 1 {
		t.Fatal("editor return discarded markings")
	}
	if _, err := os.Stat(file); !os.IsNotExist(err) {
		t.Fatal("confirmed saved working copy was not cleaned up")
	}
	// A late duplicate may not close the connection handed back to the browser.
	applyEditorMessage(t, a, request)
	if cmd := applyEditorMessage(t, a, closed); cmd != nil {
		cmd()
	}
	if _, err := conn.Stat("/file.txt"); err != nil {
		t.Fatalf("duplicate request closed live browser connection: %v", err)
	}
}

func TestEditorFailureKeepsWorkingFileAndDoesNotUpload(t *testing.T) {
	a, server := editorApp(t, false)
	t.Setenv("DRIFT_GUI_EDITOR_FAIL", "1")
	_, file, cmd := startEditorForTest(t, a)
	cmd = applyEditorMessage(t, a, runEditorCommand(t, cmd))
	if cmd != nil || a.remoteEditor.Error() == nil || a.state.Screen != ScreenRemoteEditor {
		t.Fatal("editor failure was not retained")
	}
	if cmd := applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("u")}); cmd != nil {
		t.Fatal("editor failure allowed upload")
	}
	prompt := certtrust.New(tlstrust.Challenge{}, 120, 24)
	a.certPrompt = &prompt
	if cmd := applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyCtrlC}); cmd != nil || a.state.Screen != ScreenRemoteEditor || a.certPrompt != nil {
		t.Fatal("certificate Ctrl+C bypassed working-copy protection")
	}
	cmd = applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("k")})
	applyEditorMessage(t, a, runEditorCommand(t, cmd))
	if data, err := os.ReadFile(file); err != nil || string(data) != "edited by helper" {
		t.Fatalf("failed editor lost edits: %q, %v", data, err)
	}
	if server.CommandCount("STOR") != 0 || a.state.Screen != ScreenBrowser {
		t.Fatal("keep local copy uploaded or failed to return")
	}
	if err := os.RemoveAll(filepath.Dir(file)); err != nil {
		t.Fatal(err)
	}
}

func TestRemoteEditorGUIHelper(t *testing.T) {
	if os.Getenv("DRIFT_GUI_EDITOR_HELPER") != "1" {
		return
	}
	if err := os.WriteFile(os.Args[len(os.Args)-1], []byte("edited by helper"), 0600); err != nil {
		os.Exit(2)
	}
	if os.Getenv("DRIFT_GUI_EDITOR_FAIL") == "1" {
		os.Exit(7)
	}
	os.Exit(0)
}
