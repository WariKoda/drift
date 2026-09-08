package browser

import (
	"bufio"
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"reflect"
	"runtime/pprof"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	driftftp "github.com/WariKoda/drift/internal/ftp"
	tea "github.com/charmbracelet/bubbletea"
)

func TestConnectionResultsRejectOldBrowserGenerations(t *testing.T) {
	for _, change := range []string{"different directory", "same directory", "same browser reconnect"} {
		t.Run(change, func(t *testing.T) {
			dir := t.TempDir()
			old, err := New(dir)
			if err != nil {
				t.Fatal(err)
			}
			host := config.Host{Name: "staging", Hostname: "same.example", RootPath: "/"}
			_ = old.StartRemote(host) // Identity tests never execute network commands.
			t.Cleanup(old.remoteTracker.Cancel)
			stale := MsgRemoteLoaded{
				Host: host, Root: "/old", ID: old.remoteLoadID, session: old.remoteSession,
				Conn: &driftftp.Client{}, Entries: []*fs.FileEntry{{Name: "old", Path: "/old"}},
			}
			current := old
			if change != "same browser reconnect" {
				if change == "different directory" {
					dir = t.TempDir()
				}
				current, err = New(dir)
				if err != nil {
					t.Fatal(err)
				}
				if current.remoteSession == nil || current.remoteSession == old.remoteSession {
					t.Fatal("New reused a browser session identity")
				}
			}
			_ = current.StartRemote(host)
			t.Cleanup(current.remoteTracker.Cancel)
			if change != "same browser reconnect" && current.remoteLoadID != stale.ID {
				t.Fatal("test needs colliding request IDs to exercise browser identity")
			}
			if current.AcceptsRemoteResult(stale) {
				t.Fatal("stale successful root result was accepted")
			}
			before := current
			// Do not Close a zero client. Deferred disposal is tested over TCP below.
			current.applyRemoteLoaded(stale)
			if !reflect.DeepEqual(current, before) {
				t.Fatal("stale root result changed the current load")
			}
			parent := &fs.FileEntry{Name: "folder", Path: "/folder", Kind: fs.EntryDir, Expanded: true}
			fresh := MsgRemoteLoaded{
				Host: host, Root: "/", ID: current.remoteLoadID, session: current.remoteSession,
				Conn: &driftftp.Client{}, Entries: []*fs.FileEntry{parent},
			}
			if !current.AcceptsRemoteResult(fresh) {
				t.Fatal("current root result was rejected")
			}
			current, _ = current.Update(fresh)
			if current.Connection() != fresh.Conn || current.remoteLoading {
				t.Fatal("current root result was not installed")
			}
			current.remoteReading = true
			children := MsgRemoteChildrenLoaded{
				Host: host, ID: stale.ID, session: stale.session, ParentPath: parent.Path,
				Children: []*fs.FileEntry{{Name: "child", Path: "/folder/child", Kind: fs.EntryFile}},
			}
			if current.AcceptsRemoteChildrenResult(children) {
				t.Fatal("stale successful children result was accepted")
			}
			status := current.remoteStatus
			current, cmd := current.Update(children)
			if cmd != nil || !current.remoteReading || current.remoteStatus != status || len(current.remoteEntries) != 1 || parent.Children != nil || !parent.Expanded {
				t.Fatal("stale children result changed the current directory read")
			}
			children.ID, children.session = current.remoteLoadID, current.remoteSession
			if !current.AcceptsRemoteChildrenResult(children) {
				t.Fatal("current children result was rejected")
			}
			current, _ = current.Update(children)
			if current.remoteReading || len(current.remoteEntries) != 2 || children.Children[0].Parent != parent || children.Children[0].Depth != 1 {
				t.Fatal("current children result was not installed")
			}
		})
	}
}

func TestConnectionLostPreservesLocalPreviewAndInvalidatesRemote(t *testing.T) {
	for _, source := range []PaneSide{PaneLocal, PaneRemote} {
		for _, stage := range []string{"debounce", "loaded result", "displayed"} {
			t.Run(fmt.Sprintf("pane=%d/%s", source, stage), func(t *testing.T) {
				dir := t.TempDir()
				if err := os.WriteFile(filepath.Join(dir, "file"), []byte("content"), 0o600); err != nil {
					t.Fatal(err)
				}
				m, err := New(dir)
				if err != nil {
					t.Fatal(err)
				}
				conn := &driftftp.Client{} // Identity only; no transport methods are executed.
				host := config.Host{Name: "staging"}
				m.remoteConn, m.remoteHost, m.remoteLoadID = conn, &host, 5
				entry := &fs.FileEntry{Name: "file", Path: "/file", Kind: fs.EntryFile, Mode: 0o644}
				m.remoteEntries = []*fs.FileEntry{entry}
				m.RemoteSelection.Toggle(entry.Path)
				m.Selection.Toggle(m.entries[0].Path)
				m.activePane = source
				if m.togglePreview() == nil {
					t.Fatal("preview did not schedule a debounce")
				}
				request := m.preview.pending
				result := msgPreviewLoaded{request: request, lines: []string{"content"}}
				if stage != "debounce" {
					cmd := m.beginPreviewLoad(request)
					if cmd == nil {
						t.Fatal("preview read did not start")
					}
					if source == PaneLocal {
						result = cmd().(msgPreviewLoaded)
						if result.err != nil {
							t.Fatal(result.err)
						}
					}
				}
				if stage == "displayed" {
					m, _ = m.Update(result)
				}
				before := m.preview
				m.remoteReading = true
				failure := errors.New("keep-alive connection lost")
				if m.ConnectionLost(nil, failure) || m.ConnectionLost(&driftftp.Client{}, failure) || m.ConnectionLost(conn, nil) {
					t.Fatal("unrelated or normal-close notification was accepted")
				}
				if !reflect.DeepEqual(m.preview, before) || m.remoteLoadID != 5 || !m.remoteReading {
					t.Fatal("ignored notification invalidated pending work")
				}
				if !m.ConnectionLost(conn, failure) {
					t.Fatal("owned connection loss was rejected")
				}
				if m.remoteLoadID != 6 || m.remoteReading || m.remotePreviewReading || m.Connection() != conn || m.remoteEntries[0] != entry || m.Selection.Count() != 1 || m.RemoteSelection.Count() != 1 {
					t.Fatal("loss failed to invalidate remote work or discarded visible state")
				}
				if !strings.Contains(m.remoteStatus, failure.Error()) || !strings.Contains(m.remoteStatus, "[r]") {
					t.Fatalf("missing disconnect reason or reconnect hint: %q", m.remoteStatus)
				}
				if source == PaneLocal {
					if !reflect.DeepEqual(m.preview, before) || !m.AcceptsPreviewResult(result) {
						t.Fatal("remote loss invalidated the local preview")
					}
					if stage == "debounce" {
						var cmd tea.Cmd
						m, cmd = m.Update(msgPreviewDebounced{request: request})
						if cmd == nil {
							t.Fatal("local debounce was discarded after remote loss")
						}
						result = cmd().(msgPreviewLoaded)
					}
					m, _ = m.Update(result)
					if !m.preview.loaded || m.preview.loading || !reflect.DeepEqual(m.preview.lines, []string{"content"}) {
						t.Fatal("local preview result was not displayed after remote loss")
					}
				} else {
					if m.preview.generation != before.generation+1 || m.preview.loading || m.preview.waiting || m.AcceptsPreviewResult(result) {
						t.Fatal("remote preview was not invalidated")
					}
					after := m.preview
					var cmd tea.Cmd
					m, cmd = m.Update(msgPreviewDebounced{request: request})
					if cmd != nil {
						t.Fatal("stale remote debounce started a read")
					}
					m, cmd = m.Update(result)
					if cmd != nil || !reflect.DeepEqual(m.preview, after) || !reflect.DeepEqual(m.preview.lines, before.lines) || m.preview.loaded != before.loaded || m.preview.path != before.path {
						t.Fatal("late remote result changed the retained preview")
					}
				}
			})
		}
	}
}

func TestConnectionCancelStalledRootListing(t *testing.T) {
	for _, stall := range []string{"reply", "data", "final reply"} {
		for _, action := range []string{"cancel", "close"} {
			t.Run(stall+"/"+action, func(t *testing.T) {
				waitBrowserConnectionMonitors(t, 0)
				server := startBrowserConnectionFTP(t, stall)
				m, err := New(t.TempDir())
				if err != nil {
					t.Fatal(err)
				}
				cmd := m.StartRemote(server.host)
				tracker := m.remoteTracker
				t.Cleanup(tracker.Cancel)
				id, session := m.remoteLoadID, m.remoteSession
				result := make(chan tea.Msg, 1)
				go func() { result <- cmd() }()
				waitBrowserConnectionSignal(t, server.listing)
				waitBrowserConnectionMonitors(t, 1)
				select {
				case msg := <-result:
					t.Fatalf("listing returned before cancellation: %#v", msg)
				default:
				}
				if action == "close" {
					if m.CloseRemote() != nil {
						t.Fatal("pending load has no installed connection to close via command")
					}
				} else {
					m.CancelRemote()
				}
				if !tracker.Canceled() || m.remoteLoading || m.remoteLoadID == id {
					t.Fatal("cancel/close failed to cancel and invalidate the pending load")
				}
				var msg MsgRemoteLoaded
				select {
				case value := <-result:
					msg = value.(MsgRemoteLoaded)
				case <-time.After(5 * time.Second):
					t.Fatal("cancellation did not interrupt ReadDir")
				}
				if !errors.Is(msg.Err, context.Canceled) || msg.Conn != nil || msg.ID != id || msg.session != session {
					t.Fatalf("canceled root result = %#v", msg)
				}
				waitBrowserConnectionSignal(t, server.done)
				waitBrowserConnectionMonitors(t, 0)
				status := m.remoteStatus
				m, closeCmd := m.Update(msg)
				if closeCmd != nil {
					runBrowserConnectionCmd(t, closeCmd)
				}
				if m.Connection() != nil || m.remoteLoading || m.remoteStatus != status {
					t.Fatal("canceled result changed the closed browser")
				}
			})
		}
	}
}

func TestConnectionLateSuccessReturnsCloseCommand(t *testing.T) {
	for _, action := range []string{"cancel", "close", "replace same directory", "replace different directory"} {
		t.Run(action, func(t *testing.T) {
			waitBrowserConnectionMonitors(t, 0)
			server := startBrowserConnectionFTP(t, "")
			dir := t.TempDir()
			m, err := New(dir)
			if err != nil {
				t.Fatal(err)
			}
			load := m.StartRemote(server.host)
			tracker := m.remoteTracker
			t.Cleanup(tracker.Cancel)
			msg := runBrowserConnectionCmd(t, load).(MsgRemoteLoaded)
			if msg.Err != nil || msg.Conn == nil || msg.session != m.remoteSession || msg.ID != m.remoteLoadID || len(msg.Entries) != 1 {
				t.Fatalf("root load = %#v", msg)
			}
			t.Cleanup(func() { _ = msg.Conn.Close() })
			// Both the command's deferred cancel and this explicit parent cancel
			// must be detached after ReadDir hands ownership to the result.
			tracker.Cancel()
			children := runBrowserConnectionCmd(t, readRemoteDirCmd(msg.Conn, msg.Host, msg.ID, msg.session, "/folder")).(MsgRemoteChildrenLoaded)
			if children.Err != nil || children.session != msg.session || children.ID != msg.ID || len(children.Children) != 1 {
				t.Fatalf("directory read after load-context cancellation = %#v", children)
			}
			waitBrowserConnectionSignal(t, server.probe)
			waitBrowserConnectionMonitors(t, 1)
			switch action {
			case "cancel":
				m.CancelRemote()
			case "close":
				if m.CloseRemote() != nil {
					t.Fatal("unapplied result must still own its connection")
				}
			default:
				if action == "replace different directory" {
					dir = t.TempDir()
				}
				m, err = New(dir)
				if err != nil {
					t.Fatal(err)
				}
				_ = m.StartRemote(server.host)
				t.Cleanup(m.remoteTracker.Cancel)
				if m.remoteLoadID != msg.ID {
					t.Fatal("replacement test needs colliding request IDs")
				}
			}
			before := m
			m, closeCmd := m.Update(msg)
			if closeCmd == nil || !reflect.DeepEqual(m, before) {
				t.Fatal("stale success changed the browser or failed to return a close command")
			}
			select {
			case <-msg.Conn.Done():
				t.Fatal("Update closed the stale connection on the UI thread")
			default:
			}
			runBrowserConnectionCmd(t, closeCmd)
			waitBrowserConnectionSignal(t, msg.Conn.Done())
			waitBrowserConnectionSignal(t, server.done)
			waitBrowserConnectionMonitors(t, 0)
			if msg.Conn.Err() != nil {
				t.Fatalf("stale-result disposal reported a terminal failure: %v", msg.Conn.Err())
			}
		})
	}
}

func TestConnectionCloseRemoteDefersTransportShutdown(t *testing.T) {
	waitBrowserConnectionMonitors(t, 0)
	server := startBrowserConnectionFTP(t, "")
	m, err := New(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	msg := runBrowserConnectionCmd(t, m.StartRemote(server.host)).(MsgRemoteLoaded)
	if msg.Err != nil || msg.Conn == nil {
		t.Fatalf("root load = %#v", msg)
	}
	t.Cleanup(func() { _ = msg.Conn.Close() })
	m, _ = m.Update(msg)
	m.remoteReading, m.remotePreviewReading = true, true
	id := m.remoteLoadID
	closeCmd := m.CloseRemote()
	if closeCmd == nil || m.Connection() != nil || m.remoteBusy() || m.remoteLoadID == id {
		t.Fatal("CloseRemote did not detach and invalidate immediately")
	}
	select {
	case <-msg.Conn.Done():
		t.Fatal("CloseRemote performed transport shutdown before its command ran")
	default:
	}
	waitBrowserConnectionSignal(t, server.probe)
	waitBrowserConnectionMonitors(t, 1)
	runBrowserConnectionCmd(t, closeCmd)
	waitBrowserConnectionSignal(t, msg.Conn.Done())
	waitBrowserConnectionSignal(t, server.done)
	waitBrowserConnectionMonitors(t, 0)
	if msg.Conn.Err() != nil || m.CloseRemote() != nil || m.ConnectionLost(msg.Conn, errors.New("late")) {
		t.Fatal("closed model still owns a connection or normal close reported failure")
	}
}

const browserConnectionTimeout = 5 * time.Second

func runBrowserConnectionCmd(t *testing.T, cmd tea.Cmd) tea.Msg {
	t.Helper()
	if cmd == nil {
		t.Fatal("missing command")
	}
	result := make(chan tea.Msg, 1)
	go func() { result <- cmd() }()
	select {
	case msg := <-result:
		return msg
	case <-time.After(browserConnectionTimeout):
		t.Fatal("connection command did not finish")
		return nil
	}
}

func waitBrowserConnectionSignal(t *testing.T, signal <-chan struct{}) {
	t.Helper()
	select {
	case <-signal:
	case <-time.After(browserConnectionTimeout):
		t.Fatal("connection lifecycle signal did not arrive")
	}
}

// These tests are serial. Count only FTP monitor stacks, not unrelated runtime
// goroutines. This also detects leaked monitors when a canceled load returns no
// Client and its private monitorDone channel is inaccessible from this package.
func waitBrowserConnectionMonitors(t *testing.T, want int) {
	t.Helper()
	deadline := time.Now().Add(browserConnectionTimeout)
	for {
		var stacks bytes.Buffer
		if err := pprof.Lookup("goroutine").WriteTo(&stacks, 2); err != nil {
			t.Fatal(err)
		}
		got := strings.Count(stacks.String(), "\ngithub.com/WariKoda/drift/internal/ftp.(*Client).monitor(")
		if got == want {
			return
		}
		if time.Now().After(deadline) {
			t.Fatalf("FTP monitors = %d, want %d\n%s", got, want, stacks.String())
		}
		time.Sleep(10 * time.Millisecond)
	}
}

// A single-client loopback FTP server with real passive data sockets. Stalls
// withhold wire replies or data; no remote.Client methods are replaced.
type browserConnectionFTP struct {
	host    config.Host
	listing chan struct{}
	probe   chan struct{}
	done    chan struct{}
}

func startBrowserConnectionFTP(t *testing.T, stall string) *browserConnectionFTP {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	interval := 1
	addr := listener.Addr().(*net.TCPAddr)
	s := &browserConnectionFTP{
		host: config.Host{Name: "staging", Protocol: "ftp", Hostname: addr.IP.String(), Port: addr.Port,
			User: "drift", Auth: config.Auth{Password: "test"}, RootPath: "/", KeepAliveInterval: &interval},
		listing: make(chan struct{}, 16), probe: make(chan struct{}, 16), done: make(chan struct{}),
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(func() {
		cancel()
		_ = listener.Close()
		waitBrowserConnectionSignal(t, s.done)
	})
	go func() {
		defer close(s.done)
		control, err := listener.Accept()
		if err != nil {
			return
		}
		defer control.Close()
		stop := context.AfterFunc(ctx, func() { _ = control.Close() })
		defer stop()
		reply := func(text string) bool {
			_, err := fmt.Fprintf(control, "%s\r\n", text)
			return err == nil
		}
		if !reply("220 loopback FTP") {
			return
		}
		var passive net.Listener
		defer func() {
			if passive != nil {
				_ = passive.Close()
			}
		}()
		reader := bufio.NewReader(control)
		for {
			line, err := reader.ReadString('\n')
			if err != nil {
				return
			}
			command, _, _ := strings.Cut(strings.TrimSpace(line), " ")
			switch command {
			case "USER":
				reply("331 password required")
			case "PASS":
				reply("230 logged in")
			case "FEAT":
				reply("211-Features:\r\n MLST type*;size*;modify*;\r\n211 End")
			case "TYPE":
				reply("200 binary mode")
			case "EPSV":
				if passive != nil {
					_ = passive.Close()
				}
				passive, err = net.Listen("tcp", "127.0.0.1:0")
				if err != nil {
					t.Errorf("passive listener: %v", err)
					return
				}
				reply(fmt.Sprintf("229 Passive (|||%d|)", passive.Addr().(*net.TCPAddr).Port))
			case "MLSD":
				if passive == nil {
					t.Error("MLSD without passive listener")
					return
				}
				stopAccept := context.AfterFunc(ctx, func() { _ = passive.Close() })
				data, err := passive.Accept()
				stopAccept()
				if err != nil {
					return
				}
				stopData := context.AfterFunc(ctx, func() { _ = data.Close() })
				if stall != "reply" {
					reply("150 opening data")
				}
				if stall == "reply" || stall == "data" {
					s.listing <- struct{}{}
					// A listing client sends no data. EOF proves cancellation
					// closed its passive socket as well as the control socket.
					_, _ = io.Copy(io.Discard, data)
				} else {
					_, _ = io.WriteString(data, "type=dir;modify=20240102030405; folder\r\n")
				}
				_ = data.Close()
				stopData()
				if stall != "" {
					if stall == "final reply" {
						s.listing <- struct{}{}
					}
					continue // Withhold the final reply; wait for control EOF.
				}
				reply("226 transfer complete")
			case "NOOP":
				if !reply("200 alive") {
					return
				}
				select {
				case s.probe <- struct{}{}:
				default:
				}
			case "QUIT":
				reply("221 goodbye")
				return
			default:
				reply("502 unsupported")
			}
		}
	}()
	return s
}
