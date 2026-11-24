package app

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
)

// A cloned repository can hold a directory whose name contains a line break.
// Comparing it against an FTP host must not send the rest of the name as a
// command; the file shows up as a failed comparison instead.
func TestLoadKeepsLineBreakInLocalNameOutOfFTPCommands(t *testing.T) {
	const victim = "/deploy/victim.txt"
	root := t.TempDir()
	folder := filepath.Join(root, "folder")
	hostile := filepath.Join(folder, "a\nDELE ", "deploy", "victim.txt")
	plain := filepath.Join(folder, "plain.txt")
	writeScopeFile(t, hostile, "local\n")
	writeScopeFile(t, plain, "local\n")

	server := ftptest.Start(t, 16)
	server.AddFile(victim, "keep\n")
	server.AddFile("/deploy/folder/plain.txt", "remote\n")
	host := server.Host(t)
	host.RootPath = "/deploy"
	conn := connectTestHost(t, host)
	selection := fs.NewSelectionState()
	selection.Marked[folder] = struct{}{}
	tracker := progress.NewTracker("Connecting…")
	loaded, err := Load(tracker.Context(), LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: root},
		Local: selection, Conn: conn, IdleTimeout: 5 * time.Second}, tracker)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	defer loaded.Conn.Close()
	defer loaded.Root.Close()

	if count := server.CommandCount("DELE"); count != 0 {
		t.Errorf("server received %d DELE command(s)", count)
	}
	if _, ok := server.File(victim); !ok {
		t.Error("the comparison deleted a remote file")
	}
	foundHostile, foundPlain := false, false
	for _, session := range loaded.Sessions {
		switch session.LocalPath {
		case hostile:
			foundHostile = true
			if session.Err == nil || !strings.Contains(session.Err.Error(), "control character") {
				t.Errorf("hostile file session error = %v", session.Err)
			}
		case plain:
			foundPlain = true
			if session.Err != nil || session.Result == nil || !session.Result.HasDiff() {
				t.Errorf("plain file session = %+v", session)
			}
		}
	}
	if !foundHostile || !foundPlain {
		t.Fatalf("sessions = %+v", loaded.Sessions)
	}
}
