package diffview

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
	syncpolicy "github.com/WariKoda/drift/internal/sync"
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

			server := startFTPTestServer(t, 16)
			server.addFile("/deploy/sibling.txt", "outside selection\n")
			if tc.deniedDir != "" {
				server.addFile("/deploy/folder/local.txt", "remote\n")
				server.addFile("/deploy/folder/private/secret.txt", "private\n")
				server.mu.Lock()
				server.denyCommand = func(command, argument string) bool {
					return command == "LIST" && argument == tc.deniedDir
				}
				server.mu.Unlock()
			}
			host := server.host(t)
			host.RootPath = tc.remoteRoot
			conn := connectDiffTestHost(t, host)
			defer conn.Close()
			selection := fs.NewSelectionState()
			selection.Marked[folder] = struct{}{}
			msg := loadCmdWithOptions(1, host, selection, nil,
				&config.MergedConfig{ProjectRoot: root}, conn, NewLoadProgressTracker(), nil, nil,
				syncpolicy.ScopeOptions{}, 5*time.Second)()
			loaded, ok := msg.(MsgDiffLoaded)
			if !ok {
				t.Fatalf("load result = %T: %#v", msg, msg)
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
