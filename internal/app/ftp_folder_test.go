package app

import (
	"errors"
	"net/textproto"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
)

func TestLoadLocalFolderFTPRemoteWalk(t *testing.T) {
	for _, tc := range []struct {
		name       string
		remoteRoot string
		deniedDir  string
	}{
		{name: "missing folder", remoteRoot: "/deploy"},
		{name: "missing ancestor", remoteRoot: "/deploy/missing/ancestor"},
		{name: "denied selected folder", remoteRoot: "/deploy", deniedDir: "/deploy/folder"},
		{name: "denied descendant", remoteRoot: "/deploy", deniedDir: "/deploy/folder/private"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			root := t.TempDir()
			folder := filepath.Join(root, "folder")
			localFile := filepath.Join(folder, "local.txt")
			writeScopeFile(t, localFile, "local\n")

			server := ftptest.Start(t, 16)
			server.AddFile("/deploy/sibling.txt", "outside selection\n")
			if tc.deniedDir != "" {
				server.AddFile("/deploy/folder/local.txt", "remote\n")
				server.AddFile("/deploy/folder/private/secret.txt", "private\n")
				server.SetDenyCommand(func(command, argument string) bool {
					return command == "LIST" && argument == tc.deniedDir
				})
			}
			host := server.Host(t)
			host.RootPath = tc.remoteRoot
			conn := connectTestHost(t, host)
			defer conn.Close()
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

			wantSessions := 1
			if tc.deniedDir != "" {
				wantSessions++
			}
			if len(loaded.Sessions) != wantSessions {
				t.Fatalf("sessions = %+v, want %d", loaded.Sessions, wantSessions)
			}
			foundFile, foundWalkError := false, false
			for _, session := range loaded.Sessions {
				if !session.Loaded {
					t.Fatalf("session was not loaded: %+v", session)
				}
				switch session.LocalPath {
				case localFile:
					foundFile = true
					if session.RemotePath != tc.remoteRoot+"/folder/local.txt" || session.Err != nil || session.Result == nil {
						t.Fatalf("local file session = %+v", session)
					}
					if session.Result.LocalOnly != (tc.deniedDir == "") || session.Result.RemoteOnly || !session.Result.HasDiff() {
						t.Fatalf("file result = %+v", session.Result)
					}
				case folder:
					foundWalkError = true
					var reply *textproto.Error
					if tc.deniedDir == "" || session.RemotePath != tc.remoteRoot+"/folder" ||
						!errors.As(session.Err, &reply) || reply.Code != 550 ||
						errors.Is(session.Err, os.ErrNotExist) || !strings.Contains(session.Err.Error(), "walk remote") ||
						!strings.Contains(session.Err.Error(), "permission denied") {
						t.Fatalf("folder error session = %+v", session)
					}
				default:
					t.Fatalf("unexpected session: %+v", session)
				}
			}
			if !foundFile || foundWalkError != (tc.deniedDir != "") {
				t.Fatalf("file retained = %v, walk error visible = %v", foundFile, foundWalkError)
			}
		})
	}
}
