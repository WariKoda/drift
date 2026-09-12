package diffview

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
)

func TestLoadScopeIncludesHiddenAndSkipsRecursiveIgnoredFiles(t *testing.T) {
	root := t.TempDir()
	writeScopeFile(t, filepath.Join(root, ".gitignore"), "ignored.txt\ncache/\n")
	writeScopeFile(t, filepath.Join(root, ".hidden"), "hidden")
	writeScopeFile(t, filepath.Join(root, "ignored.txt"), "ignored")
	writeScopeFile(t, filepath.Join(root, "normal.txt"), "normal")
	writeScopeFile(t, filepath.Join(root, "cache", "cached.txt"), "cached")

	server := startFTPTestServer(t, 2*maxFTPDiffLoadWorkers+4)
	host := server.host(t)
	selection := fs.NewSelectionState()
	selection.Marked[root] = struct{}{}
	cfg := &config.MergedConfig{ProjectRoot: root}

	load := func(includeIgnored bool) MsgDiffLoaded {
		conn := connectDiffTestHost(t, host)
		progress := NewLoadProgressTracker()
		msg := loadCmdWithOptions(1, host, selection, nil, cfg, conn, progress, nil, nil,
			syncpolicy.ScopeOptions{IncludeIgnored: includeIgnored}, 5*time.Second)()
		loaded, ok := msg.(MsgDiffLoaded)
		if !ok {
			t.Fatalf("load result = %T: %#v", msg, msg)
		}
		t.Cleanup(func() {
			_ = loaded.Conn.Close()
			_ = loaded.Root.Close()
		})
		return loaded
	}

	filtered := load(false)
	if filtered.Scope.Pairs != 3 || filtered.Scope.Hidden != 2 || filtered.Scope.IgnoredFilesSkipped != 1 || filtered.Scope.IgnoredDirsSkipped != 1 {
		t.Fatalf("filtered scope = %+v", filtered.Scope)
	}
	included := load(true)
	if included.Scope.Pairs != 5 || included.Scope.Hidden != 2 || included.Scope.IgnoredFilesSkipped != 0 || included.Scope.IgnoredDirsSkipped != 0 {
		t.Fatalf("included scope = %+v", included.Scope)
	}
}

func TestLoadScopeDirectIgnoredFileOverridesOnlyThatFile(t *testing.T) {
	root := t.TempDir()
	writeScopeFile(t, filepath.Join(root, ".gitignore"), "*.env\n")
	selected := filepath.Join(root, "selected.env")
	writeScopeFile(t, selected, "selected")
	writeScopeFile(t, filepath.Join(root, "neighbor.env"), "neighbor")

	server := startFTPTestServer(t, maxFTPDiffLoadWorkers)
	host := server.host(t)
	conn := connectDiffTestHost(t, host)
	selection := fs.NewSelectionState()
	selection.Marked[root] = struct{}{}
	selection.Marked[selected] = struct{}{}
	progress := NewLoadProgressTracker()
	msg := loadCmdWithOptions(2, host, selection, nil, &config.MergedConfig{ProjectRoot: root}, conn,
		progress, nil, nil, syncpolicy.ScopeOptions{}, 5*time.Second)()
	loaded, ok := msg.(MsgDiffLoaded)
	if !ok {
		t.Fatalf("load result = %T: %#v", msg, msg)
	}
	defer loaded.Conn.Close()
	defer loaded.Root.Close()
	if loaded.Scope.ExplicitIgnoredIncluded != 1 || loaded.Scope.IgnoredFilesSkipped != 1 {
		t.Fatalf("scope = %+v", loaded.Scope)
	}
}

func writeScopeFile(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}
