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

func TestRemoteBrowserRevealsLoadedDirectoryChildren(t *testing.T) {
	entries := []*fs.FileEntry{
		{Name: "a", Path: "/a", Kind: fs.EntryDir},
		{Name: "b", Path: "/b", Kind: fs.EntryDir},
		{Name: "c", Path: "/c", Kind: fs.EntryDir},
		{Name: "d", Path: "/d", Kind: fs.EntryDir},
		{Name: "target", Path: "/target", Kind: fs.EntryDir, Expanded: true},
	}
	model := Model{
		Height:        11, // five entry rows
		remoteEntries: entries,
		remoteCursor:  4,
		remoteReading: true,
	}

	model.applyRemoteChildrenLoaded(MsgRemoteChildrenLoaded{
		ParentPath: "/target",
		Children: []*fs.FileEntry{
			{Name: "one", Path: "/target/one", Kind: fs.EntryFile},
			{Name: "two", Path: "/target/two", Kind: fs.EntryFile},
			{Name: "three", Path: "/target/three", Kind: fs.EntryFile},
		},
	})

	if model.remoteOffset != 3 {
		t.Fatalf("remote offset = %d, want 3 so the directory and all children are visible", model.remoteOffset)
	}
}

func TestRemoteBrowserDoesNotJumpAfterCursorLeavesLoadingDirectory(t *testing.T) {
	entries := []*fs.FileEntry{
		{Name: "active", Path: "/active", Kind: fs.EntryDir},
		{Name: "b", Path: "/b", Kind: fs.EntryDir},
		{Name: "c", Path: "/c", Kind: fs.EntryDir},
		{Name: "d", Path: "/d", Kind: fs.EntryDir},
		{Name: "loading", Path: "/loading", Kind: fs.EntryDir, Expanded: true},
	}
	model := Model{
		Height:        11, // five entry rows
		remoteEntries: entries,
		remoteCursor:  0,
		remoteReading: true,
	}

	model.applyRemoteChildrenLoaded(MsgRemoteChildrenLoaded{
		ParentPath: "/loading",
		Children: []*fs.FileEntry{
			{Name: "one", Path: "/loading/one", Kind: fs.EntryFile},
			{Name: "two", Path: "/loading/two", Kind: fs.EntryFile},
			{Name: "three", Path: "/loading/three", Kind: fs.EntryFile},
		},
	})

	if model.remoteOffset != 0 {
		t.Fatalf("remote offset = %d, want 0 after the cursor left the loading directory", model.remoteOffset)
	}
}

func TestRemoteBrowserBlocksDirectoryReadDuringPreviewRead(t *testing.T) {
	model := Model{
		activePane:           PaneRemote,
		remoteConn:           &driftftp.Client{},
		remotePreviewReading: true,
		remoteEntries: []*fs.FileEntry{
			{Name: "directory", Path: "/directory", Kind: fs.EntryDir},
		},
	}

	next, cmd := model.updateRemoteOpen()
	if cmd != nil {
		t.Fatal("directory read started while a preview read was active")
	}
	if next.remoteReading {
		t.Fatal("blocked directory read marked the connection busy")
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

func TestRemotePreviewWaitsForDirectoryRead(t *testing.T) {
	request := previewRequest{generation: 1, source: PaneRemote, path: "/file.txt"}
	model := Model{
		activePane:    PaneRemote,
		remoteConn:    &driftftp.Client{},
		remoteReading: true,
		remoteEntries: []*fs.FileEntry{
			{Name: "file.txt", Path: request.path, Kind: fs.EntryFile},
		},
		preview: filePreview{
			active:     true,
			source:     PaneRemote,
			generation: request.generation,
			pending:    request,
		},
	}

	if cmd := model.beginPreviewLoad(request); cmd != nil {
		t.Fatal("preview read started while a directory read was active")
	}
	if !model.preview.waiting {
		t.Fatal("preview read was not queued")
	}

	model.remoteReading = false
	if cmd := model.resumePreviewLoad(); cmd == nil {
		t.Fatal("queued preview read did not start after directory read")
	}
	if !model.remotePreviewReading {
		t.Fatal("started preview read did not mark remote connection busy")
	}
}

func TestCancelRemoteIgnoresStaleResult(t *testing.T) {
	model := Model{
		remoteHost:      &config.Host{Name: "staging"},
		remoteLoading:   true,
		remoteLoadID:    1,
		Selection:       fs.NewSelectionState(),
		RemoteSelection: fs.NewSelectionState(),
	}
	model.CancelRemote()
	if model.remoteLoading {
		t.Fatal("cancel left the remote pane loading")
	}

	model.applyRemoteLoaded(MsgRemoteLoaded{
		Host: config.Host{Name: "staging"},
		ID:   1,
	})
	if model.remoteConn != nil {
		t.Fatal("cancelled connect still installed a connection")
	}
}

func TestRemoteBrowserBlocksConnectionReplacementDuringPreviewRead(t *testing.T) {
	model := Model{
		activePane:           PaneRemote,
		remoteHost:           &config.Host{Name: "test"},
		remoteConn:           &driftftp.Client{},
		remotePreviewReading: true,
		Selection:            fs.NewSelectionState(),
		RemoteSelection:      fs.NewSelectionState(),
	}

	for _, key := range []string{"r", "@", "H", "P", "s"} {
		t.Run(key, func(t *testing.T) {
			next, cmd := model.updateNormal(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)})
			if cmd != nil {
				t.Fatalf("key %q started a screen or connection change during a preview read", key)
			}
			if !next.remotePreviewReading {
				t.Fatalf("key %q cleared the active preview read", key)
			}
		})
	}
}
