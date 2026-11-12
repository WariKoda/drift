package browser

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
)

func TestLocalBrowserOpensWithUnreadableIgnoredDirectory(t *testing.T) {
	root := t.TempDir()
	writeVisibilityFile(t, filepath.Join(root, ".gitignore"), "cache/\n")
	writeVisibilityFile(t, filepath.Join(root, "source.go"), "package source\n")
	cache := filepath.Join(root, "cache")
	writeVisibilityFile(t, filepath.Join(cache, "private.txt"), "cache\n")
	if err := os.Chmod(cache, 0); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.Chmod(cache, 0o700); err != nil {
			t.Error(err)
		}
	})
	if _, err := os.ReadDir(cache); err == nil {
		t.Skip("directory permissions do not prevent reads for this user")
	} else if !errors.Is(err, os.ErrPermission) {
		t.Fatal(err)
	}

	m, err := New(root)
	if err != nil {
		t.Fatalf("open browser: %v", err)
	}
	assertVisibleNames(t, m.filteredEntries(), "source.go")
	m, _ = m.Update(m.RefreshLocal()())
	if m.statusMsg != "Refreshed" {
		t.Fatalf("refresh failed: %s", m.statusMsg)
	}
	for i, entry := range m.entries {
		if entry.Path != cache {
			continue
		}
		cmd := m.expandAt(i)
		result := cmd().(MsgLocalLoaded)
		if !errors.Is(result.err, os.ErrPermission) {
			t.Fatalf("explicit cache expansion error = %v", result.err)
		}
		m, _ = m.Update(result)
		if !strings.Contains(m.statusMsg, "Local read failed") {
			t.Fatalf("read error was not shown: %s", m.statusMsg)
		}
		return
	}
	t.Fatal("ignored cache directory was not retained in the tree")
}

func TestLocalRefreshReclassifiesCollapsedAndRemoteSelections(t *testing.T) {
	root := t.TempDir()
	file := filepath.Join(root, "dir", "file.txt")
	writeVisibilityFile(t, file, "source\n")
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	m, _ = m.Update(m.expandAt(0)())
	m.Selection.Marked[file] = struct{}{}
	m.collapseAt(0)
	host := config.Host{RootPath: "/deploy"}
	m.remoteHost = &host
	m.RemoteSelection.Marked["/deploy/remote.txt"] = struct{}{}
	m.RemoteSelection.Marked["/deploy/remote-dir"] = struct{}{}
	if _, err := m.classifier.ClassifyBatch(t.Context(), []fs.ClassifyCandidate{
		{Path: filepath.Join(root, "remote.txt")},
		{Path: filepath.Join(root, "remote-dir"), IsDir: true},
	}); err != nil {
		t.Fatal(err)
	}

	m, _ = m.Update(m.RefreshLocal()())
	if got := m.hiddenSelectionCount(); got != 0 {
		t.Fatalf("visible collapsed and remote marks counted as hidden after refresh: %d", got)
	}
	writeVisibilityFile(t, filepath.Join(root, ".gitignore"), "dir/file.txt\nremote.txt\nremote-dir/\n")
	m, _ = m.Update(m.RefreshLocal()())
	if got := m.hiddenSelectionCount(); got != 3 {
		t.Fatalf("new ignore rules not applied to marked paths: hidden = %d, want 3", got)
	}
	m.showIgnored = true
	if got := m.hiddenSelectionCount(); got != 0 {
		t.Fatalf("including ignored files did not reveal all marks: hidden = %d", got)
	}
	if !m.Selection.IsMarked(file) || len(m.RemoteSelection.Marked) != 2 {
		t.Fatal("refresh lost existing selections")
	}
}

func TestLocalRefreshReclassifiesMarksAddedWhilePending(t *testing.T) {
	root := t.TempDir()
	file := filepath.Join(root, "dir", "file.txt")
	writeVisibilityFile(t, file, "source\n")
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	indexed := buildFinderIndexCmd(root, m.classifier, false, false, 1, m.remoteSession)().(msgFinderIndex)
	if indexed.err != nil {
		t.Fatal(indexed.err)
	}
	host := config.Host{RootPath: "/deploy"}
	m.remoteHost = &host
	if _, err := m.classifier.ClassifyBatch(t.Context(), []fs.ClassifyCandidate{{Path: filepath.Join(root, "remote.txt")}}); err != nil {
		t.Fatal(err)
	}
	previous := m.classifier
	result := m.RefreshLocal()()
	m.Selection.Marked[file] = struct{}{}
	m.RemoteSelection.Marked["/deploy/remote.txt"] = struct{}{}
	m, cmd := m.Update(result)
	if cmd == nil || m.classifier != previous {
		t.Fatal("refresh adopted a classifier missing newly selected paths")
	}
	m, _ = m.Update(cmd())
	if m.classifier == previous || m.statusMsg != "Refreshed" || m.hiddenSelectionCount() != 0 {
		t.Fatal("follow-up refresh did not classify and preserve new marks")
	}
}
