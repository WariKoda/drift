package browser

import (
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
)

func TestEditRequestTransfersConnectionAndPreservesBrowserSession(t *testing.T) {
	m, server := newRemoteDirectoryBrowser(t)
	m.SetConfig(&config.MergedConfig{ProjectRoot: m.WorkDir, Editor: &config.Editor{Command: []string{"nano", "{file}"}}})
	m, read := m.updateRemoteOpen()
	m, _ = m.Update(read())
	m.remoteCursor = 1
	m.remoteOffset = 1
	entry := m.remoteCurrent()
	m.RemoteSelection.Toggle(entry.Path)
	m.preview = filePreview{active: true, source: PaneRemote, loaded: true, path: entry.Path, generation: 3}
	conn, generation := m.remoteConn, m.remoteLoadID
	m, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
	if cmd == nil || m.remoteConn != nil || m.editConn != conn || !m.editPending {
		t.Fatal("editing did not reserve the existing connection")
	}
	request := cmd().(MsgEditRequested)
	for _, key := range []string{"r", "@", "s", "H", "P", "q", "j"} {
		var cmd tea.Cmd
		m, cmd = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)})
		if cmd != nil || m.remoteCursor != 1 {
			t.Fatalf("%q raced the pending edit handoff", key)
		}
	}
	if request.Path != entry.Path || request.Conn != conn || !m.TakeEditRequest(request) || m.TakeEditRequest(request) {
		t.Fatal("edit handoff lost its snapshot or accepted a duplicate")
	}
	server.AddFile(entry.Path, "new longer content")
	info, err := conn.Stat(entry.Path)
	if err != nil {
		t.Fatal(err)
	}
	cmd = m.ResumeEdit(request, conn, info, true, "saved")
	if cmd == nil || m.remoteConn != conn || m.remoteCursor != 1 || m.remoteOffset != 1 || m.remoteLoadID == generation {
		t.Fatal("return rebuilt the browser or lost connection identity")
	}
	if !m.remoteEntries[0].Expanded || !m.RemoteSelection.IsMarked(entry.Path) || entry.Size != info.Size() || !m.preview.active || m.preview.loaded {
		t.Fatal("return lost state or failed to invalidate the saved preview")
	}
}

func TestRemoteEditGuards(t *testing.T) {
	for _, reason := range []string{"local", "directory", "symlink", "unmapped", "busy", "no connection", "no editor"} {
		t.Run(reason, func(t *testing.T) {
			m, _ := newRemoteDirectoryBrowser(t)
			m.SetConfig(&config.MergedConfig{Editor: &config.Editor{Command: []string{"nano", "{file}"}}})
			m, read := m.updateRemoteOpen()
			m, _ = m.Update(read())
			m.remoteCursor = 1
			switch reason {
			case "local":
				m.activePane = PaneLocal
			case "directory":
				m.remoteCursor = 0
			case "symlink":
				m.remoteCurrent().Kind = fs.EntrySymlink
			case "unmapped":
				m.remoteCurrent().Unmapped = true
			case "busy":
				m.remotePreviewReading = true
			case "no connection":
				m.remoteConn = nil
			case "no editor":
				m.config.Editor = nil
			}
			m, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
			if cmd != nil || m.editPending || m.statusMsg == "" {
				t.Fatalf("%s was not rejected with a reason", reason)
			}
		})
	}
}

func TestPendingEditConnectionIsClosedByBrowserShutdown(t *testing.T) {
	m, _ := newRemoteDirectoryBrowser(t)
	m.SetConfig(&config.MergedConfig{Editor: &config.Editor{Command: []string{"nano", "{file}"}}})
	m, read := m.updateRemoteOpen()
	m, _ = m.Update(read())
	m.remoteCursor = 1
	conn := m.Connection()
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
	cmd := m.CloseRemote()
	if cmd == nil {
		t.Fatal("pending edit transport leaked")
	}
	cmd()
	select {
	case <-conn.Done():
	default:
		t.Fatal("shutdown did not close pending handoff")
	}
}
