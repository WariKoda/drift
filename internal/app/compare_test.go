package app

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	stdsync "sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
)

// TestLoadDiffItemsUsesSingleFTPSession covers a server that permits only one
// session per user: connecting and browsing succeed, so the diff has to run on
// the connection that is already open instead of failing every file with a
// worker connect error.
func TestLoadDiffItemsUsesSingleFTPSession(t *testing.T) {
	localDir := t.TempDir()
	server := ftptest.Start(t, 1)
	items := make([]diffLoadItem, 6)
	for i := range items {
		localPath := filepath.Join(localDir, fmt.Sprintf("file%d.txt", i))
		if err := os.WriteFile(localPath, []byte("local\n"), 0o644); err != nil {
			t.Fatalf("write local file: %v", err)
		}
		remotePath := fmt.Sprintf("/file%d.txt", i)
		server.AddFile(remotePath, "remote\n")
		items[i] = diffLoadItem{LocalPath: localPath, RemotePath: remotePath, Compare: true}
	}

	host := server.Host(t)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host, nil, nil)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer conn.Close()

	root, err := fs.OpenRoot(localDir)
	if err != nil {
		t.Fatalf("open project root: %v", err)
	}
	defer root.Close()

	sessions, err := loadDiffItems(context.Background(), root, host, conn, items, progress.NewTracker("Connecting…"), nil, nil)
	if err != nil {
		t.Fatalf("load diff items: %v", err)
	}

	if len(sessions) != len(items) {
		t.Fatalf("sessions = %d, want %d", len(sessions), len(items))
	}
	for _, session := range sessions {
		if session.Err != nil {
			t.Fatalf("session %s: %v", session.RemotePath, session.Err)
		}
		if session.Result == nil || !session.Result.HasDiff() {
			t.Fatalf("session %s did not report the content difference", session.RemotePath)
		}
	}
	if rejected := server.RejectedSessions(); rejected != maxFTPDiffLoadWorkers-1 {
		t.Fatalf("rejected sessions = %d, want %d — extra workers must still try to connect",
			rejected, maxFTPDiffLoadWorkers-1)
	}
}

// TestForEachCompareAddsExtraFTPConnections verifies that reusing the existing
// connection does not collapse the pool to a single worker: when the server
// accepts more sessions, additional workers still connect on their own.
func TestForEachCompareAddsExtraFTPConnections(t *testing.T) {
	server := ftptest.Start(t, maxFTPDiffLoadWorkers)
	host := server.Host(t)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host, nil, nil)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer conn.Close()

	var mu stdsync.Mutex
	distinct := map[remote.Client]struct{}{}
	var once stdsync.Once
	secondWorker := make(chan struct{})

	jobs := []int{0, 1, 2, 3}
	ran := make([]bool, len(jobs))
	forEachCompare(context.Background(), host, conn, jobs, nil, nil, nil, func(idx int, workerConn remote.Client) {
		ran[idx] = true
		mu.Lock()
		distinct[workerConn] = struct{}{}
		count := len(distinct)
		mu.Unlock()
		if count > 1 {
			once.Do(func() { close(secondWorker) })
			return
		}
		// Block the first worker so the remaining jobs can only make progress
		// once a second worker has connected.
		select {
		case <-secondWorker:
		case <-time.After(5 * time.Second):
		}
	})

	for idx, done := range ran {
		if !done {
			t.Fatalf("job %d was never run", idx)
		}
	}
	mu.Lock()
	count := len(distinct)
	_, usedExisting := distinct[conn]
	mu.Unlock()
	if count < 2 {
		t.Fatalf("distinct worker connections = %d, want at least 2", count)
	}
	if !usedExisting {
		t.Fatal("no worker used the existing connection")
	}
}

func TestForEachComparePropagatesExtraWorkerTerminalFailure(t *testing.T) {
	server := ftptest.Start(t, 2)
	server.SetDropCommand(func(command, argument string) bool { return command == "SIZE" && argument == "/drop-worker" })
	host := server.Host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connectTestHost(t, host)
	released := make(chan struct{})
	var once sync.Once
	var worker remote.Client
	err := forEachCompare(context.Background(), host, conn, []int{0, 1}, nil, nil, nil, func(_ int, workerConn remote.Client) {
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

func TestCompareCancellationClosesExtraWorker(t *testing.T) {
	server := ftptest.Start(t, 2)
	host := server.Host(t)
	conn := connectTestHost(t, host)
	tracker := progress.NewTracker("Connecting…")
	released := make(chan struct{})
	var worker remote.Client
	err := forEachCompare(tracker.Context(), host, conn, []int{0, 1}, tracker, nil, nil, func(_ int, workerConn remote.Client) {
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

func TestLoadLogsScanErrorSessions(t *testing.T) {
	logPath := filepath.Join(t.TempDir(), "drift.log")
	logFile, err := log.Init(log.Options{Path: logPath})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := logFile.Close(); err != nil {
			t.Errorf("close log: %v", err)
		}
	})

	localDir := t.TempDir()
	localPath := filepath.Join(localDir, "missing local.txt")
	remotePath := "/denied remote.txt"
	server := ftptest.Start(t, 1)
	server.AddFile(remotePath, "payload")
	server.SetDenyCommand(func(_, argument string) bool { return argument == remotePath })
	host := server.Host(t)
	host.Auth.Password = "scan-log-password-must-stay-private"
	conn := connectTestHost(t, host)
	localSelection := fs.NewSelectionState()
	localSelection.Marked[localPath] = struct{}{}
	remoteSelection := fs.NewSelectionState()
	remoteSelection.Marked[remotePath] = struct{}{}
	tracker := progress.NewTracker("Scanning…")
	loaded, err := Load(tracker.Context(), LoadRequest{
		Host: host, Config: &config.MergedConfig{ProjectRoot: localDir},
		Local: localSelection, Remote: remoteSelection, Conn: conn,
	}, tracker)
	if err != nil {
		t.Fatalf("load scan failures: %v", err)
	}
	defer loaded.Root.Close()
	if len(loaded.Sessions) != 2 {
		t.Fatalf("sessions = %+v, want local and remote scan errors", loaded.Sessions)
	}
	for _, session := range loaded.Sessions {
		if session.Err == nil || !session.Loaded || session.Result != nil {
			t.Fatalf("scan error was not preserved as a loaded error session: %+v", session)
		}
		if session.RemotePath == "" && !errors.Is(session.Err, os.ErrNotExist) {
			t.Fatalf("local missing classification lost: %v", session.Err)
		}
		if session.RemotePath != "" && errors.Is(session.Err, os.ErrNotExist) {
			t.Fatalf("remote permission failure classified as missing: %v", session.Err)
		}
	}

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	output := string(data)
	if got := strings.Count(output, `msg="diff scan failed"`); got != len(loaded.Sessions) {
		t.Fatalf("scan log entries = %d, want %d:\n%s", got, len(loaded.Sessions), output)
	}
	for _, session := range loaded.Sessions {
		for _, field := range []string{
			"local=" + strconv.Quote(session.LocalPath),
			"remote=" + strconv.Quote(session.RemotePath),
			"err=" + strconv.Quote(session.Err.Error()),
		} {
			if !strings.Contains(output, field) {
				t.Errorf("scan log lacks %s:\n%s", field, output)
			}
		}
	}
	if strings.Contains(output, host.Auth.Password) {
		t.Fatal("scan log includes host credentials")
	}
}

func TestEmptyComparisonKeepsTerminalFailure(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.SetDropCommand(func(command, _ string) bool { return command == "NOOP" })
	host := server.Host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connectTestHost(t, host)
	terminal := waitTestFailure(t, conn)
	root, err := fs.OpenRoot(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	if _, err := loadDiffItems(context.Background(), root, host, conn, nil, progress.NewTracker("Connecting…"), nil, nil); !errors.Is(err, terminal) {
		t.Fatalf("empty comparison lost terminal failure: %v", err)
	}
}
