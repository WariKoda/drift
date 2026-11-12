package app

import (
	"errors"
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

func TestLoadWithUnreadableIgnoredDirectory(t *testing.T) {
	for _, tc := range []struct {
		name           string
		selected       string
		includeIgnored bool
		wantReadError  bool
	}{
		{name: "unrelated selected file", selected: "src/source.go"},
		{name: "recursive selection skips ignored cache", selected: "."},
		{name: "explicit cache reports read error", selected: "var/cache", includeIgnored: true, wantReadError: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			root := t.TempDir()
			writeScopeFile(t, filepath.Join(root, ".gitignore"), ".gitignore\nvar/cache/\n")
			file := filepath.Join(root, "src", "source.go")
			writeScopeFile(t, file, "package source\n")
			cache := filepath.Join(root, "var", "cache")
			writeScopeFile(t, filepath.Join(cache, "private.txt"), "cache\n")
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

			server := ftptest.Start(t, 16)
			server.AddFile("/src/source.go", "package remote_source\n")
			host := server.Host(t)
			selection := fs.NewSelectionState()
			selection.Marked[filepath.Join(root, filepath.FromSlash(tc.selected))] = struct{}{}
			tracker := progress.NewTracker("Connecting…")
			loaded, err := Load(tracker.Context(), LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: root},
				Local: selection, Conn: connectTestHost(t, host), Options: syncpolicy.ScopeOptions{IncludeIgnored: tc.includeIgnored},
				IdleTimeout: 5 * time.Second}, tracker)
			if err != nil {
				t.Fatalf("load: %v", err)
			}
			defer loaded.Conn.Close()
			defer loaded.Root.Close()
			if len(loaded.Sessions) != 1 {
				t.Fatalf("sessions = %+v, want one source file or explicit read error", loaded.Sessions)
			}
			session := loaded.Sessions[0]
			if tc.wantReadError {
				if session.LocalPath != cache || !errors.Is(session.Err, os.ErrPermission) {
					t.Fatalf("unreadable selected cache error was lost: %+v", session)
				}
			} else if session.LocalPath != file || session.Err != nil || session.Result == nil || !session.Result.HasDiff() {
				t.Fatalf("source comparison failed: %+v", session)
			}
		})
	}
}
