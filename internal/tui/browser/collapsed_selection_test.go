package browser

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

func TestCollapsedDirectoryCountsMarkedDescendants(t *testing.T) {
	selection := fs.NewSelectionState()
	for _, path := range []string{"/project/src", "/project/src/file.go", "/project/src/nested", "/project/src/nested/.env", "/project/src-old/other.go"} {
		selection.Toggle(path)
	}
	entry := &fs.FileEntry{Name: "src", Path: "/project/src", Kind: fs.EntryDir}
	m := Model{}
	for _, current := range []bool{false, true} {
		got := ansi.Strip(m.renderEntry(entry, current, 80, selection, ""))
		if !strings.Contains(got, "src/ · 3 marked") || !strings.Contains(got, "●") {
			t.Fatalf("current=%v: %q", current, got)
		}
	}
	entry.Expanded = true
	if got := ansi.Strip(m.renderEntry(entry, false, 80, selection, "")); strings.Contains(got, "marked") {
		t.Fatalf("expanded directory badge: %q", got)
	}
	entry.Expanded = false
	selection.Clear()
	selection.Toggle(entry.Path)
	if got := ansi.Strip(m.renderEntry(entry, false, 80, selection, "")); strings.Contains(got, "marked") {
		t.Fatalf("directory counted itself: %q", got)
	}
	if got := ansi.Strip(m.renderEntry(entry, false, 80, nil, "")); strings.Contains(got, "marked") {
		t.Fatalf("nil selection badge: %q", got)
	}
}

func TestCollapsedSelectionIndicatorSurvivesTruncation(t *testing.T) {
	entry := &fs.FileEntry{Name: strings.Repeat("日本語", 20), Path: "/project/long", Kind: fs.EntryDir, Depth: 3}
	selection := fs.NewSelectionState()
	selection.Toggle("/project/long/nested/file.txt")
	m := Model{}
	for _, width := range []int{10, 16, 24, 40, 80} {
		got := m.renderEntry(entry, true, width, selection, "")
		if lipgloss.Width(got) != width {
			t.Fatalf("width %d: %q (%d)", width, got, lipgloss.Width(got))
		}
		want := "· 1 marked"
		if width == 10 {
			want = "· 1"
		}
		if !strings.Contains(ansi.Strip(got), want) {
			t.Fatalf("badge lost at width %d: %q", width, got)
		}
	}
}

func TestCollapsedSelectionsArePaneSpecific(t *testing.T) {
	entry := &fs.FileEntry{Name: "src", Path: "/src", Kind: fs.EntryDir}
	m := Model{entries: []*fs.FileEntry{entry}, remoteEntries: []*fs.FileEntry{entry}, remoteHost: &config.Host{Name: "test"}, Selection: fs.NewSelectionState(), RemoteSelection: fs.NewSelectionState()}
	m.Selection.Toggle("/src/local.txt")
	m.RemoteSelection.Toggle("/src/remote.txt")
	m.RemoteSelection.Toggle("/src/nested/remote.txt")
	for _, pane := range []PaneSide{PaneLocal, PaneRemote} {
		m.activePane = pane
		if got := ansi.Strip(m.renderLocalRow(m.entries, 0, 80)); !strings.Contains(got, "· 1 marked") {
			t.Fatal(got)
		}
		if got := ansi.Strip(m.renderRemoteRow(0, 80)); !strings.Contains(got, "· 2 marked") {
			t.Fatal(got)
		}
	}
}

func TestCollapsePreservesAndRevealsSelectionCount(t *testing.T) {
	root := t.TempDir()
	file := filepath.Join(root, "src", "nested", "file.go")
	writeVisibilityFile(t, file, "package nested")
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	dir := m.entries[0]
	if err := m.expandAt(0); err != nil {
		t.Fatal(err)
	}
	m.Selection.Toggle(file)
	if got := ansi.Strip(m.renderLocalRow(m.filteredEntries(), 0, 80)); strings.Contains(got, "marked") {
		t.Fatalf("expanded badge: %q", got)
	}
	m.collapseAt(0)
	if got := ansi.Strip(m.renderLocalRow(m.filteredEntries(), 0, 80)); !strings.Contains(got, "· 1 marked") {
		t.Fatalf("missing collapsed badge: %q", got)
	}
	if dir.Expanded || !m.Selection.IsMarked(file) {
		t.Fatal("collapse changed selection")
	}
	m.Selection.Clear()
	if got := ansi.Strip(m.renderLocalRow(m.filteredEntries(), 0, 80)); strings.Contains(got, "marked") {
		t.Fatalf("stale badge: %q", got)
	}
}
