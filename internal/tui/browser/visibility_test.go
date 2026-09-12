package browser

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestVisibilityTogglesAreIndependentAndKeepTreeState(t *testing.T) {
	root := t.TempDir()
	writeVisibilityFile(t, filepath.Join(root, ".gitignore"), "ignored.txt\n")
	writeVisibilityFile(t, filepath.Join(root, "visible.txt"), "visible")
	writeVisibilityFile(t, filepath.Join(root, ".hidden"), "hidden")
	writeVisibilityFile(t, filepath.Join(root, "ignored.txt"), "ignored")
	writeVisibilityFile(t, filepath.Join(root, "dir", ".nested"), "nested")
	writeVisibilityFile(t, filepath.Join(root, "dir", "plain"), "plain")

	model, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	assertVisibleNames(t, model.filteredEntries(), "dir", "visible.txt")

	dir := model.filteredEntries()[0]
	if err := model.expandAt(model.localIndex(dir)); err != nil {
		t.Fatal(err)
	}
	if !dir.Expanded {
		t.Fatal("directory did not expand")
	}
	assertVisibleNames(t, model.filteredEntries(), "dir", "plain", "visible.txt")
	model.cursor = 2
	currentPath := model.filteredEntries()[model.cursor].Path

	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(".")})
	assertVisibleNames(t, model.filteredEntries(), "dir", ".nested", "plain", ".gitignore", ".hidden", "visible.txt")
	if got := model.filteredEntries()[model.cursor].Path; got != currentPath {
		t.Fatalf("cursor moved from %q to %q while revealing entries", currentPath, got)
	}
	if !dir.Expanded {
		t.Fatal("hidden toggle discarded expansion state")
	}

	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("I")})
	assertVisibleNames(t, model.filteredEntries(), "dir", ".nested", "plain", ".gitignore", ".hidden", "ignored.txt", "visible.txt")

	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(".")})
	assertVisibleNames(t, model.filteredEntries(), "dir", "plain", "ignored.txt", "visible.txt")
	if model.statusMsg != "" {
		t.Fatalf("visibility toggle left redundant status %q", model.statusMsg)
	}
}

func TestHelpTextShowsVisibilityStates(t *testing.T) {
	want := "[s]sync  [.]hidden  [I]ignored  [Tab]pane  [q]quit  [@]remote  [f]find  [?]help"
	got := HelpText(false, true, 200)
	if plain := ansi.Strip(got); plain != want {
		t.Fatalf("HelpText() = %q, want %q", plain, want)
	}
}

func TestRemoteHiddenStatusUsesRemotePathBeforeMapping(t *testing.T) {
	root := t.TempDir()
	model, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	host := config.Host{RootPath: "/srv", Mappings: []config.Mapping{{Local: "public", Remote: ".assets"}}}
	entries, err := model.classifyRemote([]*fs.FileEntry{{
		Name: "file.txt", Path: "/srv/.assets/file.txt", Kind: fs.EntryFile,
	}}, host)
	if err != nil {
		t.Fatal(err)
	}
	if !entries[0].Class.Hidden {
		t.Fatalf("remote dot-directory was not classified as hidden: %+v", entries[0].Class)
	}
}

func TestSelectionUsesVisibleProjection(t *testing.T) {
	root := t.TempDir()
	writeVisibilityFile(t, filepath.Join(root, ".first"), "hidden")
	writeVisibilityFile(t, filepath.Join(root, "second"), "visible")

	model, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeySpace})
	if !model.Selection.IsMarked(filepath.Join(root, "second")) {
		t.Fatal("Space marked the hidden backing entry instead of the visible row")
	}
}

func TestVisualSelectionMarksOnlyVisibleRange(t *testing.T) {
	root := t.TempDir()
	writeVisibilityFile(t, filepath.Join(root, ".hidden"), "hidden")
	writeVisibilityFile(t, filepath.Join(root, "a"), "a")
	writeVisibilityFile(t, filepath.Join(root, "b"), "b")
	model, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("v")})
	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyDown})
	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("v")})
	if model.Selection.Count() != 2 || model.Selection.IsMarked(filepath.Join(root, ".hidden")) {
		t.Fatalf("visual selection = %#v", model.Selection.Marked)
	}
}

func assertVisibleNames(t *testing.T, entries []*fs.FileEntry, want ...string) {
	t.Helper()
	if len(entries) != len(want) {
		t.Fatalf("visible entry count = %d, want %d: %#v", len(entries), len(want), entryNames(entries))
	}
	for i, entry := range entries {
		if entry.Name != want[i] {
			t.Fatalf("visible entries = %#v, want %#v", entryNames(entries), want)
		}
	}
}

func entryNames(entries []*fs.FileEntry) []string {
	names := make([]string, len(entries))
	for i, entry := range entries {
		names[i] = entry.Name
	}
	return names
}

func writeVisibilityFile(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}
