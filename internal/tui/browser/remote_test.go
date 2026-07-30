package browser

import (
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	driftftp "github.com/WariKoda/drift/internal/ftp"
	tea "github.com/charmbracelet/bubbletea"
)

func TestRemoteBrowserQueuesOnlyOneDirectoryRead(t *testing.T) {
	model := Model{
		activePane: PaneRemote,
		remoteConn: &driftftp.Client{},
		remoteEntries: []*fs.FileEntry{
			{Name: "one", Path: "/one", Kind: fs.EntryDir},
			{Name: "two", Path: "/two", Kind: fs.EntryDir},
		},
	}

	model, cmd := model.updateRemoteOpen()
	if cmd == nil {
		t.Fatal("first directory read did not start")
	}
	if !model.remoteReading {
		t.Fatal("first directory read did not mark the connection busy")
	}

	model.remoteCursor = 1
	model, cmd = model.updateRemoteOpen()
	if cmd != nil {
		t.Fatal("second directory read started while the connection was busy")
	}
	if model.remoteEntries[1].Expanded {
		t.Fatal("blocked directory read changed the second directory state")
	}

	model.applyRemoteChildrenLoaded(MsgRemoteChildrenLoaded{ParentPath: "/one"})
	if model.remoteReading {
		t.Fatal("completed directory read did not release the connection")
	}
}

func TestRemoteBrowserBlocksConnectionReplacementDuringDirectoryRead(t *testing.T) {
	model := Model{
		activePane:    PaneRemote,
		remoteHost:    &config.Host{Name: "test"},
		remoteConn:    &driftftp.Client{},
		remoteReading: true,
	}

	for _, key := range []string{"r", "@", "H", "P"} {
		t.Run(key, func(t *testing.T) {
			next, cmd := model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)})
			if cmd != nil {
				t.Fatalf("key %q started a screen or connection change during a directory read", key)
			}
			if !next.remoteReading {
				t.Fatalf("key %q cleared the active directory read", key)
			}
		})
	}
}
