package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/diffview"
	tea "github.com/charmbracelet/bubbletea"
)

func TestLocalTreeResultIsDeliveredWhileHostManagerIsOpen(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "dir"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "dir", "x"), []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	a, err := New(root, nil, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { a.Close() }()
	a.browser.SetSize(120, 24)
	var read tea.Cmd
	a.browser, read = a.browser.Update(tea.KeyMsg{Type: tea.KeyRight})
	a.state.Screen = ScreenHostManager
	updated, _ := a.Update(read())
	a = updated.(App)
	if a.state.Screen != ScreenHostManager || !strings.Contains(a.browser.View(), "x") {
		t.Fatal("local result was lost or changed the active screen")
	}
}

func TestReturningFromComparisonRefreshesLocalTree(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	root := t.TempDir()
	a, err := New(root, nil, nil, nil, ScreenDiffView, false)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { a.Close() }()
	a.browser.SetSize(120, 24)
	if err := os.WriteFile(filepath.Join(root, "downloaded.txt"), []byte("new"), 0o600); err != nil {
		t.Fatal(err)
	}
	updated, read := a.Update(diffview.MsgBackToBrowser{})
	a = updated.(App)
	if read == nil || strings.Contains(a.browser.View(), "downloaded.txt") {
		t.Fatal("return did not schedule an asynchronous local refresh")
	}
	updated, _ = a.Update(read())
	a = updated.(App)
	if !strings.Contains(a.browser.View(), "downloaded.txt") {
		t.Fatal("return refresh did not reveal the downloaded file")
	}
}

func TestConfirmedDeletePrunesMarksEvenWhenCompletionIsCancelled(t *testing.T) {
	a, host := connectedSessionApp(t)
	path := "/old/file.txt"
	a.browser.RemoteSelection.Toggle(path)
	var requestCmd tea.Cmd
	a.browser, requestCmd = a.browser.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("s")})
	request := requestCmd().(browser.MsgSyncRequested)
	a.state.Selection, a.state.RemoteSelection = request.Selection, request.RemoteSelection
	a.state.SelectedHost, a.state.Screen = &host, ScreenDiffView
	a.diffView = diffview.New([]diff.Session{{RemotePath: path, Result: &diff.DiffResult{RemoteOnly: true}}}, host, request.Conn, nil, 120, 24)
	a.diffView, _ = a.diffView.Update(tea.KeyMsg{Type: tea.KeySpace})
	var syncCmd tea.Cmd
	a.diffView, syncCmd = a.diffView.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("s")})
	commands := syncCmd().(tea.BatchMsg)
	result := commands[0]().(diffview.MsgBulkSyncDone)
	if result.Done != 1 || len(result.DeletedRemote) != 1 || result.DeletedRemote[0] != path {
		t.Fatalf("real delete did not report immutable confirmed paths: %+v", result)
	}
	a.diffView.CancelActivity()
	updated, _ := a.Update(result)
	*a = updated.(App)
	if a.state.RemoteSelection.IsMarked(path) || a.browser.RemoteSelection.IsMarked(path) {
		t.Fatal("cancelled completion retained a confirmed deleted path")
	}
	if a.state.Screen != ScreenDiffView || a.diffView.Connection() != request.Conn {
		t.Fatal("cancelled completion reconnected or left the comparison")
	}
}
