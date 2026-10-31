package browser

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/ftptest"
	tea "github.com/charmbracelet/bubbletea"
)

func TestRemoteDirectoryCancelKeysInterruptStalledRead(t *testing.T) {
	for _, key := range []tea.KeyType{tea.KeyEsc, tea.KeyCtrlC} {
		for _, overlay := range []string{"none", "finder", "filter"} {
			t.Run(tea.KeyMsg{Type: key}.String()+"/"+overlay, func(t *testing.T) {
				model, server := newRemoteDirectoryBrowser(t)
				started, release := stallRemoteDirectory(t, server)
				model.Selection.Toggle("/project/marked.txt")
				model.RemoteSelection.Toggle("/folder/one.txt")
				model, cmd := model.updateRemoteOpen()
				tracker, conn, id := model.remoteTracker, model.Connection(), model.remoteLoadID
				result := make(chan tea.Msg, 1)
				go func() { result <- cmd() }()
				waitBrowserConnectionSignal(t, started)

				model.finder.active = overlay == "finder"
				model.filterMode = overlay == "filter"
				model, closeCmd := model.Update(tea.KeyMsg{Type: key})
				if closeCmd == nil || model.Connection() != nil || model.remoteBusy() || model.remoteLoadID == id || model.remoteTracker != nil || !tracker.Canceled() {
					t.Fatal("cancel key did not detach and invalidate the directory read")
				}
				if model.remoteEntries[0].Expanded || model.remoteReadPath != "" || model.Selection.Count() != 1 || model.RemoteSelection.Count() != 1 {
					t.Fatal("cancellation left the directory expanded or discarded selections")
				}
				if model.finder.active != (overlay == "finder") || model.filterMode != (overlay == "filter") {
					t.Fatal("overlay consumed the cancel key before the remote read")
				}
				runBrowserConnectionCmd(t, closeCmd)
				waitBrowserConnectionSignal(t, conn.Done())
				var msg MsgRemoteChildrenLoaded
				select {
				case value := <-result:
					msg = value.(MsgRemoteChildrenLoaded)
				case <-time.After(browserConnectionTimeout):
					t.Fatal("cancel key did not interrupt ReadDir")
				}
				release()
				if !errors.Is(msg.Err, context.Canceled) || msg.conn != conn || msg.tracker != tracker || msg.ID != id || len(msg.Children) != 0 {
					t.Fatalf("cancelled directory result = %+v", msg)
				}
				before := model
				model, cmd = model.Update(msg)
				if cmd != nil || !reflect.DeepEqual(model, before) {
					t.Fatal("cancelled result changed the browser")
				}
			})
		}
	}
}

func TestRemoteDirectoryDeadlineInterruptsStalledRead(t *testing.T) {
	model, server := newRemoteDirectoryBrowser(t)
	started, release := stallRemoteDirectory(t, server)
	model, _ = model.updateRemoteOpen()
	tracker, conn, id := model.remoteTracker, model.Connection(), model.remoteLoadID
	request := MsgRemoteChildrenLoaded{
		Host: *model.remoteHost, ID: id, session: model.remoteSession,
		ParentPath: model.remoteReadPath, conn: conn, tracker: tracker,
	}
	// A shorter parent deadline exercises the command's timeout path without
	// making the regression suite wait for the production 30-second limit.
	ctx, cancel := context.WithTimeout(tracker.Context(), time.Second)
	defer cancel()
	result := make(chan tea.Msg, 1)
	go func() { result <- readRemoteDirCmd(ctx, request, model.classifier, model.config, model.WorkDir)() }()
	waitBrowserConnectionSignal(t, started)
	model.preview = filePreview{active: true, source: PaneRemote, generation: 5, loading: true, waiting: true}

	var msg MsgRemoteChildrenLoaded
	select {
	case value := <-result:
		msg = value.(MsgRemoteChildrenLoaded)
	case <-time.After(browserConnectionTimeout):
		t.Fatal("deadline did not interrupt ReadDir")
	}
	release()
	if !errors.Is(msg.Err, context.DeadlineExceeded) || !model.AcceptsRemoteChildrenResult(msg) {
		t.Fatalf("timed out directory result = %+v", msg)
	}
	waitBrowserConnectionSignal(t, conn.Done())
	model, cmd := model.Update(msg)
	if cmd != nil || model.Connection() != nil || model.remoteBusy() || model.remoteTracker != nil || !tracker.Canceled() || model.remoteLoadID == id {
		t.Fatal("timeout retained the closed connection or busy state")
	}
	if model.remoteEntries[0].Expanded || model.remoteReadPath != "" || model.preview.loading || model.preview.waiting {
		t.Fatal("timeout left an expanded directory or queued preview")
	}
	if !strings.Contains(model.remoteStatus, "deadline exceeded") || !strings.Contains(model.remoteStatus, "[r]") {
		t.Fatalf("missing timeout reason or reconnect hint: %q", model.remoteStatus)
	}
}

func TestRemoteDirectoryCompletionDetachesCancellationAndRejectsReplay(t *testing.T) {
	model, _ := newRemoteDirectoryBrowser(t)
	model, cmd := model.updateRemoteOpen()
	tracker, conn := model.remoteTracker, model.Connection()
	first := runBrowserConnectionCmd(t, cmd).(MsgRemoteChildrenLoaded)
	if first.Err != nil || len(first.Children) != 1 {
		t.Fatalf("directory read = %+v", first)
	}
	tracker.Cancel()
	select {
	case <-conn.Done():
		t.Fatal("completed read did not detach its context watcher")
	default:
	}
	model, _ = model.Update(first)
	if model.remoteBusy() || model.remoteTracker != nil || model.Connection() != conn || !model.remoteEntries[0].Expanded {
		t.Fatal("successful directory result did not release the read and keep the connection")
	}
	model, _ = model.updateRemoteClose()
	model, cmd = model.updateRemoteOpen()
	if model.remoteTracker == tracker {
		t.Fatal("a later directory read reused the operation identity")
	}
	if model.AcceptsRemoteChildrenResult(first) {
		t.Fatal("old completion was accepted by a newer read on the same connection")
	}
	before := model
	model, replayCmd := model.Update(first)
	if replayCmd != nil || !reflect.DeepEqual(model, before) {
		t.Fatal("replayed result changed the newer directory read")
	}
	second := runBrowserConnectionCmd(t, cmd).(MsgRemoteChildrenLoaded)
	if second.Err != nil {
		t.Fatal(second.Err)
	}
	model, _ = model.Update(second)
	if model.remoteBusy() || len(model.remoteEntries[0].Children) != 1 {
		t.Fatal("newer read did not finish normally")
	}
}

func TestRemoteDirectoryLateSuccessAfterCancelCannotReplaceNewRead(t *testing.T) {
	model, server := newRemoteDirectoryBrowser(t)
	model, cmd := model.updateRemoteOpen()
	oldConn := model.Connection()
	late := runBrowserConnectionCmd(t, cmd).(MsgRemoteChildrenLoaded)
	if late.Err != nil {
		t.Fatal(late.Err)
	}
	// The watcher has already detached, but the result has not reached Update.
	// Cancellation must still return a command to close the owned connection.
	model, closeCmd := model.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if closeCmd == nil {
		t.Fatal("cancelling an undelivered success returned no close command")
	}
	runBrowserConnectionCmd(t, closeCmd)
	waitBrowserConnectionSignal(t, oldConn.Done())
	loaded := runBrowserConnectionCmd(t, model.StartRemote(server.Host(t))).(MsgRemoteLoaded)
	if loaded.Err != nil {
		t.Fatal(loaded.Err)
	}
	t.Cleanup(func() { _ = loaded.Conn.Close() })
	model, _ = model.Update(loaded)
	model, cmd = model.updateRemoteOpen()
	for _, stale := range []MsgRemoteChildrenLoaded{late, {
		Host: *model.remoteHost, ID: model.remoteLoadID, session: model.remoteSession,
		ParentPath: model.remoteReadPath, conn: oldConn, tracker: model.remoteTracker,
		Children: late.Children,
	}} {
		if model.AcceptsRemoteChildrenResult(stale) {
			t.Fatal("old connection result was accepted by a replacement read")
		}
		before := model
		var staleCmd tea.Cmd
		model, staleCmd = model.Update(stale)
		if staleCmd != nil || !reflect.DeepEqual(model, before) {
			t.Fatal("late result changed the replacement read")
		}
	}
	fresh := runBrowserConnectionCmd(t, cmd).(MsgRemoteChildrenLoaded)
	if fresh.Err != nil {
		t.Fatal(fresh.Err)
	}
	model, _ = model.Update(fresh)
	if model.remoteBusy() || model.Connection() != loaded.Conn || len(model.remoteEntries[0].Children) != 1 {
		t.Fatal("replacement read failed to install its own result")
	}
}

func TestRemoteDirectoryTeardownInterruptsStalledRead(t *testing.T) {
	for _, action := range []string{"cancel", "close", "clear", "connection loss"} {
		t.Run(action, func(t *testing.T) {
			model, server := newRemoteDirectoryBrowser(t)
			started, release := stallRemoteDirectory(t, server)
			model, cmd := model.updateRemoteOpen()
			tracker, conn := model.remoteTracker, model.Connection()
			result := make(chan tea.Msg, 1)
			go func() { result <- cmd() }()
			waitBrowserConnectionSignal(t, started)
			switch action {
			case "cancel":
				cmd = model.CancelRemote()
			case "close":
				cmd = model.CloseRemote()
			case "clear":
				cmd = model.ClearRemote()
			case "connection loss":
				if err := conn.Close(); err != nil {
					t.Fatal(err)
				}
				if !model.ConnectionLost(conn, errors.New("connection lost")) {
					t.Fatal("owned connection loss was rejected")
				}
				cmd = nil
			}
			if cmd != nil {
				runBrowserConnectionCmd(t, cmd)
			}
			if !tracker.Canceled() || model.remoteBusy() || model.remoteTracker != nil || model.remoteReadPath != "" {
				t.Fatal("teardown did not cancel and clear the directory operation")
			}
			select {
			case value := <-result:
				before := model
				model, cmd = model.Update(value)
				if cmd != nil || !reflect.DeepEqual(model, before) {
					t.Fatal("late teardown result changed the browser")
				}
			case <-time.After(browserConnectionTimeout):
				t.Fatal("teardown did not interrupt ReadDir")
			}
			release()
			waitBrowserConnectionSignal(t, conn.Done())
		})
	}
}

func TestCancelRemoteDirectoryLateSuccessReturnsCloseCommand(t *testing.T) {
	model, _ := newRemoteDirectoryBrowser(t)
	model, cmd := model.updateRemoteOpen()
	conn := model.Connection()
	late := runBrowserConnectionCmd(t, cmd).(MsgRemoteChildrenLoaded)
	if late.Err != nil {
		t.Fatal(late.Err)
	}
	closeCmd := model.CancelRemote()
	if closeCmd == nil || model.Connection() != nil || model.remoteBusy() {
		t.Fatal("CancelRemote did not detach the connection and return its close command")
	}
	select {
	case <-conn.Done():
		t.Fatal("CancelRemote closed a completed read's connection on the UI thread")
	default:
	}
	runBrowserConnectionCmd(t, closeCmd)
	waitBrowserConnectionSignal(t, conn.Done())
	before := model
	model, cmd = model.Update(late)
	if cmd != nil || !reflect.DeepEqual(model, before) {
		t.Fatal("late success changed the cancelled browser")
	}
}

func newRemoteDirectoryBrowser(t *testing.T) (Model, *ftptest.Server) {
	t.Helper()
	server := ftptest.Start(t, 3)
	server.AddFile("/folder/one.txt", "one")
	server.AddFile("/other/two.txt", "two")
	model, err := New(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	loaded := runBrowserConnectionCmd(t, model.StartRemote(server.Host(t))).(MsgRemoteLoaded)
	if loaded.Err != nil || loaded.Conn == nil {
		t.Fatalf("root load = %+v", loaded)
	}
	t.Cleanup(func() { _ = loaded.Conn.Close() })
	model, _ = model.Update(loaded)
	model.activePane = PaneRemote
	return model, server
}

func stallRemoteDirectory(t *testing.T, server *ftptest.Server) (<-chan struct{}, func()) {
	t.Helper()
	started, resume := make(chan struct{}), make(chan struct{})
	release := sync.OnceFunc(func() { close(resume) })
	t.Cleanup(release)
	server.SetDropCommand(func(command, argument string) bool {
		if command != "LIST" || argument != "/folder" {
			return false
		}
		close(started)
		<-resume // Withhold the real FTP reply until the client cancels or times out.
		return true
	})
	return started, release
}
