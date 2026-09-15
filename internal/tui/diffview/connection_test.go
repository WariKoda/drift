package diffview

import (
	"context"
	"errors"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
)

func connectDiffTestHost(t *testing.T, host config.Host) remote.Client {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host, nil, nil)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	return conn
}

func waitDiffTestFailure(t *testing.T, conn remote.Client) error {
	t.Helper()
	select {
	case <-conn.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("connection monitor did not report failure")
	}
	if conn.Err() == nil {
		t.Fatal("connection closed without a terminal error")
	}
	return conn.Err()
}

func TestConnectionLostMatchesOwnerAndPreservesView(t *testing.T) {
	server := startFTPTestServer(t, 2)
	conn := connectDiffTestHost(t, server.host(t))
	other := connectDiffTestHost(t, server.host(t))
	sessions := []diff.Session{
		{LocalPath: "/file", Result: &diff.DiffResult{ContentDiff: true}},
		{LocalPath: "/second", Result: &diff.DiffResult{ContentDiff: true}},
	}
	model := New(sessions, server.host(t), conn, nil, 160, 24)
	model.completed = map[int]bool{0: true}
	model.syncDirs[0] = DirNone
	reason := errors.New("keep-alive peer closed")
	if model.ConnectionLost(other, reason) || model.ConnectionLost(nil, reason) || model.ConnectionLost(conn, nil) {
		t.Fatal("stale or normal-close notification applied")
	}
	if !model.ConnectionLost(conn, reason) || model.ConnectionLost(conn, reason) {
		t.Fatal("matching loss must apply exactly once")
	}
	if model.Connection() != conn || model.sessions[0].Result != sessions[0].Result || !model.completed[0] {
		t.Fatal("loss discarded the connection, comparison, or confirmed success")
	}
	for _, key := range []string{"u", "d", "s", "S", "r", " ", "A"} {
		next, cmd := model.handleKey(keyMsg(key))
		if cmd != nil || next.remoteBusy() || next.syncDirs[0] != DirNone {
			t.Fatalf("%q changed sync state on the disconnected screen", key)
		}
	}
	click := tea.MouseMsg{X: 1, Y: bodyTop, Button: tea.MouseButtonLeft, Action: tea.MouseActionPress}
	model, _ = model.updateMouse(click)
	model, cmd := model.updateMouse(click)
	if cmd != nil || model.syncDirs[0] != DirNone {
		t.Fatal("double click changed a disconnected file's sync direction")
	}
	model, _ = model.Update(MsgRefreshed{Conn: conn})
	if len(model.sessions) != 2 || !model.completed[0] {
		t.Fatal("late refresh replaced the retained comparison")
	}
	model, _ = model.handleKey(keyMsg("n"))
	if model.activeIdx != 1 {
		t.Fatal("disconnect blocked local navigation")
	}
	model.syncStatus = "another operation completed"
	if view := model.View(); !strings.Contains(view, "Disconnected: keep-alive peer closed") || !strings.Contains(view, "Reopen comparison") {
		t.Fatalf("persistent disconnect reason missing: %s", view)
	}
	if !strings.Contains(model.renderFileRow(0, 40), "✓") {
		t.Fatal("confirmed success is not visible in the file list")
	}
}

func TestCancelActivityInterruptsStalledDownload(t *testing.T) {
	server := startFTPTestServer(t, 1)
	server.addFile("/file.txt", "content")
	started := make(chan struct{})
	server.mu.Lock()
	server.sendData = func(data net.Conn, _ string) error {
		close(started)
		_, err := io.Copy(io.Discard, data)
		return err
	}
	server.mu.Unlock()

	conn := connectDiffTestHost(t, server.host(t))
	root, err := fs.OpenRoot(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = root.Close() })
	model := New([]diff.Session{{
		LocalPath: filepath.Join(root.Base(), "file.txt"), RemotePath: "/file.txt",
	}}, server.host(t), conn, root, 80, 24)
	model.quickSyncing = true
	model.beginActivity("Downloading", 1)

	result := make(chan tea.Msg, 1)
	go func() { result <- model.downloadCmd(0)() }()
	select {
	case <-started:
	case <-time.After(5 * time.Second):
		t.Fatal("download did not reach the stalled data transfer")
	}
	model.CancelActivity()

	var msg tea.Msg
	select {
	case msg = <-result:
	case <-time.After(5 * time.Second):
		t.Fatal("CancelActivity did not interrupt the download")
	}
	syncErr, ok := msg.(MsgSyncError)
	if !ok || !errors.Is(syncErr.Err, context.Canceled) {
		t.Fatalf("download result = %#v, want a canceled sync error", msg)
	}
	model, _ = model.Update(msg)
	if model.remoteBusy() || model.connectionError() == nil {
		t.Fatal("canceled download kept the model busy or reusable")
	}
}

func TestTerminalClientBlocksCommandsBeforeRootNotification(t *testing.T) {
	server := startFTPTestServer(t, 1)
	server.mu.Lock()
	server.dropCommand = func(command, _ string) bool { return command == "NOOP" }
	server.mu.Unlock()
	host := server.host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connectDiffTestHost(t, host)
	terminal := waitDiffTestFailure(t, conn)
	rootDir := t.TempDir()
	root, err := fs.OpenRoot(rootDir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	model := New([]diff.Session{{LocalPath: filepath.Join(rootDir, "file"), RemotePath: "/file", Result: &diff.DiffResult{ContentDiff: true}}}, host, conn, root, 160, 24)
	for _, key := range []string{"u", "d", "s", "S", "r"} {
		_, cmd := model.handleKey(keyMsg(key))
		if cmd != nil {
			t.Fatalf("%q scheduled network work before root observed terminal failure", key)
		}
	}
	// Commands queued before notification must also check Err at execution time.
	model.beginActivity("test", 1)
	model.syncProgress = model.activityTracker
	if msg := model.uploadCmd(0)().(MsgSyncError); !errors.Is(msg.Err, terminal) {
		t.Fatalf("upload error = %v", msg.Err)
	}
	if msg := model.downloadCmd(0)().(MsgSyncError); !errors.Is(msg.Err, terminal) {
		t.Fatalf("download error = %v", msg.Err)
	}
	if msg := model.refreshCmd()().(MsgRefreshed); !errors.Is(msg.Err, terminal) {
		t.Fatalf("refresh error = %v", msg.Err)
	}
	if msg := model.reloadSessionCmd(0)().(MsgSessionReloaded); !errors.Is(msg.Err, terminal) {
		t.Fatalf("reload error = %v", msg.Err)
	}
	model.syncDirs[0] = DirDeleteRemote
	if msg := model.bulkSyncCmd([]int{0})().(MsgBulkSyncDone); msg.Done != 0 || !errors.Is(msg.Err, terminal) {
		t.Fatalf("bulk result = %+v", msg)
	}
	if server.commandCount("DELE") != 0 || server.commandCount("RETR") != 0 || server.commandCount("SIZE") != 0 {
		t.Fatal("a queued command touched the failed connection")
	}
	if _, err := loadDiffItems(root, host, conn, nil, NewLoadProgressTracker(), nil, nil); !errors.Is(err, terminal) {
		t.Fatalf("empty comparison lost terminal failure: %v", err)
	}
	msg := LoadCmd(42, host, nil, nil, &config.MergedConfig{ProjectRoot: rootDir}, conn, NewLoadProgressTracker(), nil, nil)()
	failure, ok := msg.(MsgDiffError)
	if !ok || !errors.Is(failure.Err, terminal) || failure.RequestID != 42 {
		t.Fatalf("load result = %#v", msg)
	}
	operationErr := errors.New("delete reply lost")
	uncertain := syncOperationError(conn, operationErr)
	if !errors.Is(uncertain, terminal) || !errors.Is(uncertain, operationErr) || !strings.Contains(uncertain.Error(), "outcome unknown") {
		t.Fatalf("uncertainty lost error causes: %v", uncertain)
	}
}

func TestLateSyncCompletionKeepsConfirmedOutcomes(t *testing.T) {
	server := startFTPTestServer(t, 1)
	server.addFile("/good", "content")
	host := server.host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connectDiffTestHost(t, host)
	model := New([]diff.Session{{RemotePath: "/good"}, {RemotePath: "/missing"}}, host, conn, nil, 160, 24)
	model.syncDirs = []SyncDir{DirDeleteRemote, DirDeleteRemote}
	model.syncing = true
	model.syncProgress = model.beginActivity("Syncing", 2)
	msg := model.bulkSyncCmd([]int{0, 1})().(MsgBulkSyncDone)
	if msg.Done != 1 || len(msg.Completed) != 1 || len(msg.Errors) != 1 {
		t.Fatalf("real delete results = %+v", msg)
	}
	server.mu.Lock()
	server.dropCommand = func(command, _ string) bool { return command == "NOOP" }
	server.mu.Unlock()
	terminal := waitDiffTestFailure(t, conn)
	model.ConnectionLost(conn, terminal)
	model, cmd := model.Update(msg)
	if cmd != nil || model.remoteBusy() || !model.completed[0] || model.syncDirs[0] != DirNone || len(model.syncErrors) != 1 {
		t.Fatalf("late bulk completion lost outcomes or started refresh: %+v", model)
	}
	if model.Connection() != conn {
		t.Fatal("completion detached an unclosed connection")
	}

	// A confirmed quick operation arriving after the loss must also stay successful.
	model.quickSyncing = true
	model.beginActivity("Syncing", 1)
	model, cmd = model.Update(MsgSynced{Conn: conn, SessionIdx: 1, Direction: DirUpload})
	if cmd != nil || model.quickSyncing || !model.completed[1] {
		t.Fatal("late quick completion was lost or started a reload")
	}
	uncertain := syncOperationError(conn, errors.New("server reply lost"))
	model.activeIdx = 0
	model, _ = model.Update(MsgSyncError{Conn: conn, SessionIdx: 1, Err: uncertain})
	if !errors.Is(model.sessions[1].Err, terminal) || model.sessions[0].Err != nil || !model.completed[0] {
		t.Fatal("late uncertainty error was lost or applied to the wrong file")
	}
}

func TestForEachComparePropagatesExtraWorkerTerminalFailure(t *testing.T) {
	server := startFTPTestServer(t, 2)
	server.mu.Lock()
	server.dropCommand = func(command, argument string) bool { return command == "SIZE" && argument == "/drop-worker" }
	server.mu.Unlock()
	host := server.host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connectDiffTestHost(t, host)
	released := make(chan struct{})
	var once sync.Once
	var worker remote.Client
	err := forEachCompare(host, conn, []int{0, 1}, nil, nil, nil, func(_ int, workerConn remote.Client) {
		if workerConn == conn {
			select {
			case <-released:
			case <-time.After(5 * time.Second):
			}
			return
		}
		worker = workerConn // only one extra worker; read after the pool joins
		_, _ = workerConn.Stat("/drop-worker")
		select {
		case <-workerConn.Done():
		case <-time.After(5 * time.Second):
		}
		once.Do(func() { close(released) })
	})
	if worker == nil || worker.Err() == nil || !errors.Is(err, worker.Err()) {
		t.Fatalf("extra worker terminal failure reduced parallelism instead of failing: %v", err)
	}
	if conn.Err() != nil {
		t.Fatalf("primary connection unexpectedly failed: %v", conn.Err())
	}
}

func TestCloseWaitsForRunningCommandBeforeClosingRoot(t *testing.T) {
	server := startFTPTestServer(t, 1)
	conn := connectDiffTestHost(t, server.host(t))
	dir := t.TempDir()
	root, err := fs.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	model := New(nil, server.host(t), conn, root, 80, 24)
	started, release, finished := make(chan struct{}), make(chan struct{}), make(chan struct{})
	unblock := sync.OnceFunc(func() { close(release) })
	defer unblock()
	cmd := model.trackCommand(func() tea.Msg {
		close(started)
		<-release
		return nil
	})
	go func() { cmd(); close(finished) }()
	<-started
	closeCmd := model.Close()
	closed := make(chan struct{})
	go func() { closeCmd(); close(closed) }()
	select {
	case <-conn.Done():
	case <-time.After(3 * time.Second):
		t.Fatal("transport close did not run")
	}
	if _, err := root.Stat(dir); err != nil {
		t.Fatalf("root closed before running command exited: %v", err)
	}
	select {
	case <-closed:
		t.Fatal("cleanup finished while a command was still running")
	default:
	}
	unblock()
	<-finished
	select {
	case <-closed:
	case <-time.After(3 * time.Second):
		t.Fatal("cleanup did not finish after command exited")
	}
}

func TestCloseDetachesImmediatelyAndRejectsQueuedCommand(t *testing.T) {
	server := startFTPTestServer(t, 1)
	conn := connectDiffTestHost(t, server.host(t))
	rootDir := t.TempDir()
	path := filepath.Join(rootDir, "file")
	if err := os.WriteFile(path, []byte("local"), 0o600); err != nil {
		t.Fatal(err)
	}
	root, err := fs.OpenRoot(rootDir)
	if err != nil {
		t.Fatal(err)
	}
	model := New([]diff.Session{{LocalPath: path, RemotePath: "/file"}}, server.host(t), conn, root, 100, 24)
	model.refreshing = true
	tracker := model.beginActivity("Refreshing", 1)
	queued := model.reloadSessionCmd(0)
	closeCmd := model.Close()
	if closeCmd == nil || model.Connection() != nil || model.root != nil || model.remoteBusy() || !tracker.Canceled() {
		t.Fatal("Close did not detach and cancel immediately")
	}
	select {
	case <-conn.Done():
		t.Fatal("Close performed network cleanup before its command ran")
	default:
	}
	closed := make(chan struct{})
	go func() { closeCmd(); close(closed) }()
	select {
	case <-conn.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("Close command did not close the connection")
	}
	select {
	case <-closed:
	case <-time.After(5 * time.Second):
		t.Fatal("Close waited for a command Bubble Tea might never execute")
	}
	late := queued()
	if late != nil {
		t.Fatal("queued command executed after its resources were closed")
	}
	if conn.Err() != nil {
		t.Fatalf("normal cleanup reported a terminal failure: %v", conn.Err())
	}
	if _, cmd := model.Update(late); cmd != nil {
		t.Fatal("a late result restarted work on the closed model")
	}
	if model.Close() != nil || model.ConnectionLost(conn, errors.New("late")) {
		t.Fatal("closed model still owns connection resources")
	}
}

func TestCompareCancellationClosesExtraWorker(t *testing.T) {
	server := startFTPTestServer(t, 2)
	host := server.host(t)
	conn := connectDiffTestHost(t, host)
	tracker := NewLoadProgressTracker()
	released := make(chan struct{})
	var worker remote.Client
	err := forEachCompare(host, conn, []int{0, 1}, tracker, nil, nil, func(_ int, workerConn remote.Client) {
		if workerConn == conn {
			select {
			case <-released:
			case <-time.After(5 * time.Second):
			}
			return
		}
		worker = workerConn
		tracker.Cancel()
		select {
		case <-workerConn.Done():
		case <-time.After(5 * time.Second):
		}
		close(released)
	})
	if !errors.Is(err, context.Canceled) || worker == nil {
		t.Fatalf("worker cancellation = %v, worker = %v", err, worker)
	}
	if conn.Err() != nil || worker.Err() != nil {
		t.Fatal("cancellation was reported as a terminal failure")
	}
	select {
	case <-conn.Done():
		t.Fatal("canceling extra workers closed the owned primary connection")
	default:
	}
}

func TestStaleActivityResultsDoNotChangeNewConnection(t *testing.T) {
	server := startFTPTestServer(t, 2)
	old := connectDiffTestHost(t, server.host(t))
	current := connectDiffTestHost(t, server.host(t))
	failure := errors.New("old connection failed")
	messages := []tea.Msg{
		MsgBulkSyncDone{Conn: old, Completed: []int{0}, Done: 1, Err: failure},
		MsgSynced{Conn: old, SessionIdx: 0},
		MsgSyncError{Conn: old, SessionIdx: 0, Err: failure},
		MsgRefreshed{Conn: old},
		MsgSessionReloaded{Conn: old, SessionIdx: 0, Err: failure},
	}
	for _, msg := range messages {
		result := &diff.DiffResult{ContentDiff: true}
		model := New([]diff.Session{{Result: result}}, server.host(t), current, nil, 100, 24)
		model.quickSyncing = true
		model.beginActivity("Current operation", 1)
		next, cmd := model.Update(msg)
		if cmd != nil || next.Connection() != current || next.disconnected != nil || !next.quickSyncing || next.sessions[0].Result != result || next.sessions[0].Err != nil || next.completed[0] {
			t.Fatalf("stale %T changed the current activity", msg)
		}
	}
}

func TestUncertainBulkOutcomeRemainsVisibleWithLongCause(t *testing.T) {
	model := Model{syncErrors: []SyncFailure{{
		Operation: "upload", Path: "/file", Reason: "outcome unknown; compare again before syncing: " + strings.Repeat("long transport cause ", 10),
	}}}
	rows := model.renderErrorListRows(4, 80)
	if !strings.Contains(strings.Join(rows, "\n"), "outcome unknown") {
		t.Fatal("truncation hid the uncertain outcome warning")
	}
}
