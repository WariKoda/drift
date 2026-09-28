package app

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
)

func TestLoadScopeIncludesHiddenAndSkipsRecursiveIgnoredFiles(t *testing.T) {
	root := t.TempDir()
	writeScopeFile(t, filepath.Join(root, ".gitignore"), "ignored.txt\ncache/\n")
	writeScopeFile(t, filepath.Join(root, ".hidden"), "hidden")
	writeScopeFile(t, filepath.Join(root, "ignored.txt"), "ignored")
	writeScopeFile(t, filepath.Join(root, "normal.txt"), "normal")
	writeScopeFile(t, filepath.Join(root, "cache", "cached.txt"), "cached")

	server := ftptest.Start(t, 2*maxFTPDiffLoadWorkers+4)
	host := server.Host(t)
	selection := fs.NewSelectionState()
	selection.Marked[root] = struct{}{}
	cfg := &config.MergedConfig{ProjectRoot: root}

	load := func(includeIgnored bool) LoadResult {
		conn := connectTestHost(t, host)
		progress := progress.NewTracker("Connecting…")
		loaded, err := Load(progress.Context(), LoadRequest{Host: host, Config: cfg, Local: selection, Conn: conn,
			Options: syncpolicy.ScopeOptions{IncludeIgnored: includeIgnored}, IdleTimeout: 5 * time.Second}, progress)
		if err != nil {
			t.Fatalf("load: %v", err)
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

	server := ftptest.Start(t, maxFTPDiffLoadWorkers)
	host := server.Host(t)
	conn := connectTestHost(t, host)
	selection := fs.NewSelectionState()
	selection.Marked[root] = struct{}{}
	selection.Marked[selected] = struct{}{}
	progress := progress.NewTracker("Connecting…")
	loaded, err := Load(progress.Context(), LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: root}, Local: selection, Conn: conn,
		IdleTimeout: 5 * time.Second}, progress)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	defer loaded.Conn.Close()
	defer loaded.Root.Close()
	if loaded.Scope.ExplicitIgnoredIncluded != 1 || loaded.Scope.IgnoredFilesSkipped != 1 {
		t.Fatalf("scope = %+v", loaded.Scope)
	}
}

func TestLoadScopeSkipsInterruptedStagingFiles(t *testing.T) {
	root := t.TempDir()
	writeScopeFile(t, filepath.Join(root, "assets", ".local.bin.drift-tmp-00112233445566778899aabbccddeeff"), "partial local")

	server := ftptest.Start(t, maxFTPDiffLoadWorkers)
	server.AddFile("/assets/.big.bin.drift-tmp-ed75fbcd7e7466c701845a0190e6ae09", "partial remote")
	server.AddFile("/assets/remote.txt", "remote")
	host := server.Host(t)
	conn := connectTestHost(t, host)
	selection := fs.NewSelectionState()
	selection.Marked[root] = struct{}{}
	tracker := progress.NewTracker("Connecting…")
	loaded, err := Load(tracker.Context(), LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: root},
		Local: selection, Conn: conn, Options: syncpolicy.ScopeOptions{IncludeIgnored: true}, IdleTimeout: 5 * time.Second}, tracker)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	defer loaded.Conn.Close()
	defer loaded.Root.Close()
	if len(loaded.Sessions) != 1 || loaded.Sessions[0].RemotePath != "/assets/remote.txt" {
		t.Fatalf("sessions = %+v, want only the regular remote file", loaded.Sessions)
	}
	if loaded.Scope.HardExcludedSkipped != 2 {
		t.Fatalf("scope = %+v, want both staging files counted as fixed exclusions", loaded.Scope)
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
