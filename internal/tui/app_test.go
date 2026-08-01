package tui

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
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
