package app

import (
	"path"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
)

func TestLoadLocalMappingSelectionFindsRemoteOnlyFiles(t *testing.T) {
	plugin := config.Mapping{Local: "plugins/plugin1", Remote: "html/custom/plugins/plugin1"}
	for _, tc := range []struct {
		name         string
		mappings     []config.Mapping
		hostMappings []config.Mapping
		selected     []string
		remoteRoot   string
		wantRemote   []string
		wantListings int
	}{
		{name: "mapping ancestor", mappings: []config.Mapping{plugin}, selected: []string{"plugins"}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "project root", mappings: []config.Mapping{plugin}, selected: []string{"."}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "direct mapping root", mappings: []config.Mapping{plugin}, selected: []string{"plugins/plugin1"}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "multiple mappings and segment boundaries", mappings: []config.Mapping{plugin,
			{Local: "plugins/plugin2", Remote: "html/custom/plugins/plugin2"},
			{Local: "plugins-extra", Remote: "outside"}}, selected: []string{"plugins"}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt", "html/custom/plugins/plugin2/remote-only.txt"}, wantListings: 2},
		{name: "host mappings override project mappings", mappings: []config.Mapping{{Local: "plugins/plugin1", Remote: "outside"}},
			hostMappings: []config.Mapping{plugin}, selected: []string{"plugins"}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "overlapping selections", mappings: []config.Mapping{plugin}, selected: []string{"plugins", "plugins/plugin1"}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "nested mappings", mappings: []config.Mapping{plugin,
			{Local: "plugins/plugin1/nested", Remote: "html/custom/plugins/plugin1/nested"}}, selected: []string{"plugins"}, remoteRoot: "/deploy",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt", "html/custom/plugins/plugin1/nested/remote-only.txt"}, wantListings: 2},
		{name: "omitted remote root", mappings: []config.Mapping{plugin}, selected: []string{"plugins"},
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "absolute remote root", mappings: []config.Mapping{plugin}, selected: []string{"plugins"}, remoteRoot: "/",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "relative remote root", mappings: []config.Mapping{plugin}, selected: []string{"plugins"}, remoteRoot: ".",
			wantRemote: []string{"html/custom/plugins/plugin1/remote-only.txt"}, wantListings: 1},
		{name: "no mappings", selected: []string{"plugins/plugin1"}, remoteRoot: "/deploy",
			wantRemote: []string{"plugins/plugin1/remote-only.txt"}, wantListings: 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			root := t.TempDir()
			localFile := filepath.Join(root, "plugins", "plugin1", "local.txt")
			writeScopeFile(t, localFile, "local\n")
			writeScopeFile(t, filepath.Join(root, "plugins", "plugin1", ".gitignore"), ".gitignore\nignored.txt\n")
			selection := fs.NewSelectionState()
			for _, selected := range tc.selected {
				selection.Marked[filepath.Join(root, filepath.FromSlash(selected))] = struct{}{}
			}

			server := ftptest.Start(t, 16)
			remoteRoot := tc.remoteRoot
			if remoteRoot == "" {
				remoteRoot = "/"
			}
			for _, relative := range []string{
				"html/custom/plugins/plugin1/remote-only.txt",
				"html/custom/plugins/plugin1/ignored.txt",
				"html/custom/plugins/plugin1/.local.bin.drift-tmp-00112233445566778899aabbccddeeff",
				"html/custom/plugins/plugin2/remote-only.txt",
				"outside/remote-only.txt",
				"plugins/plugin1/remote-only.txt",
			} {
				server.AddFile(path.Join(remoteRoot, relative), "remote\n")
			}
			if tc.name == "nested mappings" {
				server.AddFile(path.Join(remoteRoot, "html/custom/plugins/plugin1/nested/remote-only.txt"), "nested\n")
			}
			host := server.Host(t)
			host.RootPath, host.Mappings = tc.remoteRoot, tc.hostMappings
			tracker := progress.NewTracker("Connecting…")
			loaded, err := Load(tracker.Context(), LoadRequest{Host: host,
				Config: &config.MergedConfig{ProjectRoot: root, Mappings: tc.mappings}, Local: selection,
				Conn: connectTestHost(t, host), IdleTimeout: 5 * time.Second}, tracker)
			if err != nil {
				t.Fatalf("load: %v", err)
			}
			defer loaded.Conn.Close()
			defer loaded.Root.Close()

			if len(loaded.Sessions) != 1+len(tc.wantRemote) {
				t.Fatalf("sessions = %+v, want one local-only and %d remote-only files", loaded.Sessions, len(tc.wantRemote))
			}
			want := make(map[string]bool)
			for _, relative := range tc.wantRemote {
				want[path.Join(remoteRoot, relative)] = true
			}
			localSeen := false
			for _, session := range loaded.Sessions {
				if !session.Loaded || session.Err != nil || session.Result == nil {
					t.Fatalf("failed session: %+v", session)
				}
				if session.LocalPath == localFile {
					localSeen = true
					if !session.Result.LocalOnly || session.Result.RemoteOnly {
						t.Fatalf("local file result = %+v", session.Result)
					}
					continue
				}
				if !want[session.RemotePath] || !session.Result.RemoteOnly || session.Result.LocalOnly {
					t.Fatalf("unexpected remote-only session: %+v", session)
				}
				delete(want, session.RemotePath)
			}
			if !localSeen || len(want) != 0 {
				t.Fatalf("local file seen = %t, missing remote files = %v", localSeen, want)
			}
			// Stat probes the missing local.txt counterpart with a LIST of the
			// file and its parent, in addition to the directory walks.
			if got, want := server.CommandCount("LIST"), tc.wantListings+2; got != want {
				t.Fatalf("remote listings = %d, want %d without overlapping walks", got, want)
			}
		})
	}
}
