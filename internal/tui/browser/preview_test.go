package browser

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestReadPreviewTextSanitizesUnsafeContent(t *testing.T) {
	lines, err := readPreviewText(bytes.NewReader([]byte("one\r\ntwo\t\x1b[31mred\a\xff\n")), 0)
	if err != nil {
		t.Fatalf("read preview text: %v", err)
	}

	want := []string{"one", "two    [31mred�"}
	if len(lines) != len(want) {
		t.Fatalf("got %d lines, want %d: %#v", len(lines), len(want), lines)
	}
	for index := range want {
		if lines[index] != want[index] {
			t.Errorf("line %d = %q, want %q", index+1, lines[index], want[index])
		}
	}
}

func TestReadPreviewTextRejectsUnsupportedContent(t *testing.T) {
	t.Run("known size", func(t *testing.T) {
		_, err := readPreviewText(strings.NewReader("small"), previewMaxBytes+1)
		if !errors.Is(err, errPreviewTooLarge) {
			t.Fatalf("got %v, want preview size error", err)
		}
	})

	t.Run("stream size", func(t *testing.T) {
		_, err := readPreviewText(bytes.NewReader(bytes.Repeat([]byte{'x'}, previewMaxBytes+1)), 0)
		if !errors.Is(err, errPreviewTooLarge) {
			t.Fatalf("got %v, want preview size error", err)
		}
	})

	t.Run("binary", func(t *testing.T) {
		_, err := readPreviewText(bytes.NewReader([]byte("text\x00binary")), 0)
		if !errors.Is(err, errPreviewBinary) {
			t.Fatalf("got %v, want binary preview error", err)
		}
	})
}

func TestLocalPreviewDoesNotFollowSymlink(t *testing.T) {
	dir := t.TempDir()
	targetPath := filepath.Join(dir, "target.txt")
	linkPath := filepath.Join(dir, "link.txt")
	if err := os.WriteFile(targetPath, []byte("secret"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(targetPath, linkPath); err != nil {
		t.Fatal(err)
	}

	result := readLocalPreviewCmd(previewRequest{source: PaneLocal, path: linkPath})().(msgPreviewLoaded)
	if !errors.Is(result.err, errPreviewNotFile) {
		t.Fatalf("symlink preview error = %v, want regular-file error", result.err)
	}
	if result.lines != nil {
		t.Fatalf("symlink target was read: %#v", result.lines)
	}
}

func TestPreviewShowsOnlyLatestSelectedFile(t *testing.T) {
	dir := t.TempDir()
	firstPath := filepath.Join(dir, "first.txt")
	secondPath := filepath.Join(dir, "second.txt")
	if err := os.WriteFile(firstPath, []byte("first"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(secondPath, []byte("second"), 0o600); err != nil {
		t.Fatal(err)
	}

	model := previewTestModel([]*fs.FileEntry{
		{Name: "first.txt", Path: firstPath, Kind: fs.EntryFile, Size: 5},
		{Name: "second.txt", Path: secondPath, Kind: fs.EntryFile, Size: 6},
	})
	_ = model.togglePreview()
	firstRequest := model.preview.pending

	model.cursor = 1
	_ = model.schedulePreview()
	secondRequest := model.preview.pending

	if cmd := model.beginPreviewLoad(firstRequest); cmd != nil {
		t.Fatal("stale debounce result started a file read")
	}
	stale := readLocalPreviewCmd(firstRequest)().(msgPreviewLoaded)
	_ = model.applyPreviewLoaded(stale)
	if model.preview.loaded {
		t.Fatal("stale preview result was displayed")
	}
	if !model.preview.loading {
		t.Fatal("stale preview result stopped the current load")
	}

	loadCmd := model.beginPreviewLoad(secondRequest)
	if loadCmd == nil {
		t.Fatal("latest debounce result did not start a file read")
	}
	latest := loadCmd().(msgPreviewLoaded)
	_ = model.applyPreviewLoaded(latest)
	if !model.preview.loaded {
		t.Fatal("latest preview result was not displayed")
	}
	if model.preview.path != secondPath {
		t.Fatalf("preview path = %q, want %q", model.preview.path, secondPath)
	}
	if len(model.preview.lines) != 1 || model.preview.lines[0] != "second" {
		t.Fatalf("preview lines = %#v, want second file content", model.preview.lines)
	}
}

func TestPreviewUsesVisibleFilteredEntry(t *testing.T) {
	model := previewTestModel([]*fs.FileEntry{
		{Name: "hidden.txt", Path: "/project/hidden.txt", Kind: fs.EntryFile},
		{Name: "visible.txt", Path: "/project/visible.txt", Kind: fs.EntryFile},
	})
	model.filter = "visible"
	model.preview = filePreview{active: true, source: PaneLocal}

	request, ok := model.currentPreviewRequest(1)
	if !ok {
		t.Fatal("visible filtered file was not previewable")
	}
	if request.path != "/project/visible.txt" {
		t.Fatalf("preview path = %q, want visible filtered entry", request.path)
	}
}

func TestRefreshReloadsActiveLocalPreview(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "version.txt")
	if err := os.WriteFile(filePath, []byte("version=1"), 0o600); err != nil {
		t.Fatal(err)
	}
	model, err := New(dir)
	if err != nil {
		t.Fatal(err)
	}
	model.preview = filePreview{
		active: true,
		source: PaneLocal,
		loaded: true,
		path:   filePath,
		lines:  []string{"version=1"},
	}
	model.layoutPreview(false)
	if err := os.WriteFile(filePath, []byte("version=2"), 0o600); err != nil {
		t.Fatal(err)
	}

	model, debounceCmd := model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(keyR)})
	if debounceCmd == nil {
		t.Fatal("refresh did not schedule a preview reload")
	}
	loadCmd := model.beginPreviewLoad(model.preview.pending)
	if loadCmd == nil {
		t.Fatal("preview reload did not start")
	}
	result := loadCmd().(msgPreviewLoaded)
	_ = model.applyPreviewLoaded(result)
	if len(model.preview.lines) != 1 || model.preview.lines[0] != "version=2" {
		t.Fatalf("refreshed preview lines = %#v", model.preview.lines)
	}
	if model.preview.offset != 0 {
		t.Fatalf("refreshed preview offset = %d, want 0", model.preview.offset)
	}
}

func TestPreviewKeepsLastContentOnDirectory(t *testing.T) {
	model := previewTestModel([]*fs.FileEntry{
		{Name: "file.txt", Path: "/project/file.txt", Kind: fs.EntryFile},
		{Name: "directory", Path: "/project/directory", Kind: fs.EntryDir},
	})
	model.preview = filePreview{
		active: true,
		source: PaneLocal,
		loaded: true,
		path:   "/project/file.txt",
		lines:  []string{"content"},
	}
	model.layoutPreview(false)

	model.cursor = 1
	if cmd := model.schedulePreview(); cmd != nil {
		t.Fatal("directory selection scheduled a preview read")
	}
	if !model.preview.loaded || model.preview.path != "/project/file.txt" {
		t.Fatal("directory selection discarded the last preview")
	}
	if model.preview.loading {
		t.Fatal("directory selection left the preview loading")
	}
}

func TestPreviewWrapsLinesAndNumbersOnlyTheirFirstRow(t *testing.T) {
	model := previewTestModel(nil)
	model.Width = 31
	model.preview = filePreview{
		active: true,
		source: PaneLocal,
		loaded: true,
		path:   "/project/file.txt",
		lines:  []string{"abcdefghijklmnopqrst"},
	}
	model.layoutPreview(false)

	if len(model.preview.rows) != 2 {
		t.Fatalf("got %d wrapped rows, want 2", len(model.preview.rows))
	}
	if model.preview.rows[0].continuation {
		t.Fatal("first wrapped row marked as continuation")
	}
	if !model.preview.rows[1].continuation {
		t.Fatal("second wrapped row not marked as continuation")
	}

	first := ansi.Strip(model.renderPreviewRow(0, 15))
	second := ansi.Strip(model.renderPreviewRow(1, 15))
	if !strings.Contains(first, "1 │") {
		t.Fatalf("first row has no line number: %q", first)
	}
	if strings.Contains(second, "1 │") {
		t.Fatalf("continuation repeats line number: %q", second)
	}
}

func TestPreviewScrollAndTabLifecycle(t *testing.T) {
	lines := make([]string, 10)
	for index := range lines {
		lines[index] = "line"
	}
	model := previewTestModel(nil)
	model.Height = 10
	model.preview = filePreview{
		active: true,
		source: PaneLocal,
		loaded: true,
		lines:  lines,
	}
	model.layoutPreview(false)

	model.scrollPreview(keyEnd)
	if model.preview.offset != 6 {
		t.Fatalf("end offset = %d, want 6", model.preview.offset)
	}
	model.scrollPreview(keyPgUp)
	if model.preview.offset != 2 {
		t.Fatalf("page-up offset = %d, want 2", model.preview.offset)
	}
	model.scrollPreview(keyHome)
	if model.preview.offset != 0 {
		t.Fatalf("home offset = %d, want 0", model.preview.offset)
	}

	model.remoteHost = nil
	model, _ = model.updateNormal(tea.KeyMsg{Type: tea.KeyTab})
	if model.preview.active {
		t.Fatal("tab did not disable preview mode")
	}
}

func TestPreviewRendersOverOppositePane(t *testing.T) {
	model := previewTestModel([]*fs.FileEntry{
		{Name: "selected.txt", Path: "/project/selected.txt", Kind: fs.EntryFile},
	})
	model.preview = filePreview{
		active: true,
		source: PaneLocal,
		loaded: true,
		path:   "/project/selected.txt",
		lines:  []string{"version=1.2.3"},
	}
	model.layoutPreview(false)

	view := ansi.Strip(model.View())
	if !strings.Contains(view, "PREVIEW") || !strings.Contains(view, "/project/selected.txt") {
		t.Fatalf("preview header missing path:\n%s", view)
	}
	if !strings.Contains(view, "1 │ version=1.2.3") {
		t.Fatalf("preview content or line number missing:\n%s", view)
	}
	if strings.Contains(view, "press [@] to choose remote host") {
		t.Fatalf("remote tree was not covered by preview:\n%s", view)
	}
}

func TestRemotePreviewRendersOverLocalPane(t *testing.T) {
	model := previewTestModel([]*fs.FileEntry{
		{Name: "local-tree.txt", Path: "/project/local-tree.txt", Kind: fs.EntryFile},
	})
	model.activePane = PaneRemote
	model.remoteHost = &config.Host{Name: "production"}
	model.remoteRoot = "/srv/app"
	model.remoteEntries = []*fs.FileEntry{
		{Name: "remote-tree.txt", Path: "/srv/app/remote-tree.txt", Kind: fs.EntryFile},
	}
	model.preview = filePreview{
		active: true,
		source: PaneRemote,
		loaded: true,
		path:   "/srv/app/remote-tree.txt",
		lines:  []string{"remote-content"},
	}
	model.layoutPreview(false)

	view := ansi.Strip(model.View())
	if strings.Contains(view, "local-tree.txt") {
		t.Fatalf("local tree was not covered by preview:\n%s", view)
	}
	if !strings.Contains(view, "1 │ remote-content") {
		t.Fatalf("remote preview content missing from local pane:\n%s", view)
	}
	if !strings.Contains(view, "remote-tree.txt") {
		t.Fatalf("remote tree disappeared while previewing it:\n%s", view)
	}
}

func TestPreviewErrorUsesPlaceholderWithoutPreviousContent(t *testing.T) {
	model := previewTestModel(nil)
	model.preview = filePreview{
		active:     true,
		source:     PaneLocal,
		generation: 3,
		loading:    true,
	}
	request := previewRequest{generation: 3, source: PaneLocal, path: "/project/large.txt"}
	_ = model.applyPreviewLoaded(msgPreviewLoaded{request: request, err: errPreviewTooLarge})

	if model.preview.loading {
		t.Fatal("failed preview remained loading")
	}
	if !strings.Contains(model.preview.message, "exceeds 1 MiB") {
		t.Fatalf("placeholder = %q", model.preview.message)
	}
	if model.preview.path != request.path {
		t.Fatalf("placeholder path = %q, want %q", model.preview.path, request.path)
	}
}

func previewTestModel(entries []*fs.FileEntry) Model {
	return Model{
		entries:         entries,
		Selection:       fs.NewSelectionState(),
		RemoteSelection: fs.NewSelectionState(),
		Width:           80,
		Height:          12,
	}
}
