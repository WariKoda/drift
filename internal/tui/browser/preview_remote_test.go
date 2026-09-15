package browser

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
)

func TestRemotePreviewCancellationInterruptsStalledRead(t *testing.T) {
	for _, stall := range []string{"reply", "data", "final reply"} {
		t.Run(stall, func(t *testing.T) {
			server := startBrowserConnectionFTP(t, stall)
			conn, err := remote.Connect(context.Background(), server.host, nil, nil)
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = conn.Close() })

			ctx, cancel := context.WithCancel(context.Background())
			request := previewRequest{source: PaneRemote, path: "/file.txt", session: new(string)}
			result := make(chan msgPreviewLoaded, 1)
			go func() { result <- readRemotePreviewCmd(ctx, conn, request)().(msgPreviewLoaded) }()
			waitBrowserConnectionSignal(t, server.listing)
			cancel()

			select {
			case msg := <-result:
				if !errors.Is(msg.err, context.Canceled) {
					t.Fatalf("preview error = %v, want context cancellation", msg.err)
				}
			case <-time.After(5 * time.Second):
				t.Fatal("cancellation did not interrupt the remote preview")
			}
			waitBrowserConnectionSignal(t, server.done)
		})
	}
}

func TestRemotePreviewReleasesCompletedReadAfterSelectionChanges(t *testing.T) {
	for _, sequence := range []string{"before debounce", "after debounce", "failed read", "multiple files", "directory", "disable", "reopen"} {
		t.Run(sequence, func(t *testing.T) {
			server := startBrowserConnectionFTP(t, "")
			m, err := New(t.TempDir())
			if err != nil {
				t.Fatal(err)
			}
			loaded := runBrowserConnectionCmd(t, m.StartRemote(server.host)).(MsgRemoteLoaded)
			if loaded.Err != nil {
				t.Fatal(loaded.Err)
			}
			t.Cleanup(func() { _ = loaded.Conn.Close() })
			m, _ = m.Update(loaded)
			m.activePane = PaneRemote
			m.remoteEntries = []*fs.FileEntry{
				{Name: "a.txt", Path: "/a.txt", Kind: fs.EntryFile, Mode: 0o644},
				{Name: "b.txt", Path: "/b.txt", Kind: fs.EntryFile, Mode: 0o644},
				{Name: "c.txt", Path: "/c.txt", Kind: fs.EntryFile, Mode: 0o644},
				{Name: "folder", Path: "/folder", Kind: fs.EntryDir},
			}
			if sequence == "failed read" {
				m.remoteEntries[0].Path = "/missing.txt"
			}
			_ = m.togglePreview()
			first := runBrowserConnectionCmd(t, m.beginPreviewLoad(m.preview.pending)).(msgPreviewLoaded)
			if (first.err != nil) != (sequence == "failed read") || !m.remotePreviewReading {
				t.Fatalf("first preview: err=%v busy=%v", first.err, m.remotePreviewReading)
			}
			// Hold the actual network result until after the user changes selection.
			// This controls event ordering without sleeps or transport mocks.
			if sequence == "disable" || sequence == "reopen" {
				_ = m.disablePreview()
				if sequence == "reopen" {
					_ = m.togglePreview()
					if m.remoteConn != nil || m.remoteBusy() {
						t.Fatal("reopening a cancelled preview retained the closing connection")
					}
					return
				}
			} else {
				m.remoteCursor = 1
				if sequence == "directory" {
					m.remoteCursor = 3
				}
				_ = m.schedulePreview()
			}
			queued := sequence == "after debounce" || sequence == "failed read" || sequence == "multiple files" || sequence == "reopen"
			if queued {
				m, _ = m.Update(msgPreviewDebounced{request: m.preview.pending})
				if !m.preview.waiting {
					t.Fatal("next preview was not queued behind the active read")
				}
			}
			if sequence == "multiple files" {
				m.remoteCursor = 2
				_ = m.schedulePreview()
				m, _ = m.Update(msgPreviewDebounced{request: m.preview.pending})
			}
			var next tea.Cmd
			m, next = m.Update(first)
			if m.preview.loaded || m.statusMsg != "" {
				t.Fatal("obsolete content or error was displayed")
			}
			if !queued {
				if m.remoteBusy() || next != nil {
					t.Fatal("obsolete completion left remote busy or bypassed the debounce")
				}
				if sequence == "directory" || sequence == "disable" {
					return
				}
				m, next = m.Update(msgPreviewDebounced{request: m.preview.pending})
			}
			if next == nil || !m.remotePreviewReading || m.preview.waiting {
				t.Fatal("latest preview did not start after the previous read completed")
			}
			// Replaying an older completion must not release the newer read.
			m, duplicateCmd := m.Update(first)
			if !m.remotePreviewReading || duplicateCmd != nil {
				t.Fatal("old completion released the newer preview read")
			}
			last := runBrowserConnectionCmd(t, next).(msgPreviewLoaded)
			if last.err != nil {
				t.Fatal(last.err)
			}
			m, _ = m.Update(last)
			want := m.remoteCurrent().Path
			if m.remoteBusy() || m.preview.loading || !m.preview.loaded || m.preview.path != want || len(m.preview.lines) != 1 || m.preview.lines[0] != want {
				t.Fatalf("latest preview not displayed or connection still busy: path=%q want=%q busy=%v", m.preview.path, want, m.remoteBusy())
			}
		})
	}
}

func TestRemotePreviewRefreshUsesNewConnectionIdentity(t *testing.T) {
	server := startBrowserConnectionFTP(t, "")
	m, err := New(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	loaded := runBrowserConnectionCmd(t, m.StartRemote(server.host)).(MsgRemoteLoaded)
	if loaded.Err != nil {
		t.Fatal(loaded.Err)
	}
	t.Cleanup(func() { _ = loaded.Conn.Close() })
	m, _ = m.Update(loaded)
	m.activePane = PaneRemote
	m.remoteEntries = []*fs.FileEntry{{Name: "a.txt", Path: "/a.txt", Kind: fs.EntryFile, Mode: 0o644}}
	_ = m.togglePreview()
	first := runBrowserConnectionCmd(t, m.beginPreviewLoad(m.preview.pending)).(msgPreviewLoaded)
	if first.err != nil {
		t.Fatal(first.err)
	}
	m, _ = m.Update(first)
	m.prepareRemotePreviewRefresh()
	// Replace the transport with another real connection, keeping the queued
	// refresh just as StartRemote does when handling the refresh key.
	runBrowserConnectionCmd(t, m.CloseRemote())
	replacement := startBrowserConnectionFTP(t, "")
	fresh := runBrowserConnectionCmd(t, m.StartRemote(replacement.host)).(MsgRemoteLoaded)
	if fresh.Err != nil {
		t.Fatal(fresh.Err)
	}
	t.Cleanup(func() { _ = fresh.Conn.Close() })
	m, cmd := m.Update(fresh)
	result := runBrowserConnectionCmd(t, cmd).(msgPreviewLoaded)
	if result.err != nil || result.request.remoteID != m.remoteLoadID || result.request.host.Port != replacement.host.Port {
		t.Fatalf("refresh used an obsolete connection identity: request=%+v err=%v", result.request, result.err)
	}
	m, _ = m.Update(result)
	if m.remoteBusy() || m.preview.loading || !m.preview.loaded || m.preview.path != "/a.txt" {
		t.Fatal("refreshed preview was rejected or left the connection busy")
	}
}
