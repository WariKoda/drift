package sync

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
)

func connect(t *testing.T, host config.Host) remote.Client {
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

func openRoot(t *testing.T) (string, *fs.Root) {
	t.Helper()
	dir := t.TempDir()
	root, err := fs.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = root.Close() })
	return dir, root
}

func TestRunAppliesEachDecision(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.AddFile("/pulled", "remote content")
	server.AddFile("/stale", "old")
	// Refuse the staged upload of "pushed" to get a protocol failure reply.
	server.SetDenyCommand(func(command, argument string) bool {
		return command == "STOR" && strings.Contains(argument, ".pushed.")
	})
	conn := connect(t, server.Host(t))
	dir, root := openRoot(t)
	obsolete := filepath.Join(dir, "obsolete")
	pushed := filepath.Join(dir, "pushed")
	for _, file := range []string{obsolete, pushed} {
		if err := os.WriteFile(file, []byte("local"), 0600); err != nil {
			t.Fatal(err)
		}
	}
	items := []Item{
		{LocalPath: filepath.Join(dir, "pulled"), RemotePath: "/pulled", Decision: DecisionDownload},
		{LocalPath: filepath.Join(dir, "stale"), RemotePath: "/stale", Decision: DecisionDeleteRemote},
		{LocalPath: obsolete, RemotePath: "/obsolete", Decision: DecisionDeleteLocal},
		{LocalPath: filepath.Join(dir, "skipped"), RemotePath: "/skipped", Decision: DecisionNone},
		{LocalPath: pushed, RemotePath: "/pushed", Decision: DecisionUpload},
	}
	tracker := progress.NewTracker("Syncing files…")
	result := Run(context.Background(), conn, root, items, tracker)

	if result.Err != nil {
		t.Fatalf("run error = %v", result.Err)
	}
	if want := []int{0, 1, 2}; !slices.Equal(result.Completed, want) {
		t.Fatalf("completed = %v, want %v", result.Completed, want)
	}
	if len(result.Failures) != 1 {
		t.Fatalf("failures = %+v, want one", result.Failures)
	}
	failure := result.Failures[0]
	if failure.Operation != "upload" || failure.Path != pushed || !strings.HasPrefix(failure.Reason, "5") {
		t.Fatalf("upload failure = %+v, want the protocol reply as reason", failure)
	}
	if data, err := os.ReadFile(filepath.Join(dir, "pulled")); err != nil || string(data) != "remote content" {
		t.Fatalf("download wrote %q, %v", data, err)
	}
	if _, ok := server.File("/stale"); ok {
		t.Fatal("remote delete left the file")
	}
	if _, err := os.Stat(obsolete); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("local delete left the file: %v", err)
	}
	if p, _ := tracker.Snapshot(); p.Done != len(items) {
		t.Fatalf("progress done = %d, want %d", p.Done, len(items))
	}
}

// Uploads write a staging file and rename it into place. A connection lost
// before the rename must leave the target untouched, report the outcome as
// unknown and not try again.
func TestRunLeavesTargetIntactWhenConnectionDropsBeforeRename(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.AddFile("/file", "old")
	server.SetDropCommand(func(command, _ string) bool { return command == "RNFR" })
	conn := connect(t, server.Host(t))
	dir, root := openRoot(t)
	local := filepath.Join(dir, "file")
	if err := os.WriteFile(local, []byte("new"), 0o600); err != nil {
		t.Fatal(err)
	}

	result := Run(context.Background(), conn, root, []Item{{LocalPath: local, RemotePath: "/file", Decision: DecisionUpload}}, nil)

	if len(result.Completed) != 0 || len(result.Failures) != 1 {
		t.Fatalf("result = %+v, want one failed upload", result)
	}
	if !strings.Contains(result.Failures[0].Err.Error(), "outcome unknown") || result.Err == nil {
		t.Fatalf("failure = %v, run error = %v; want an unknown outcome on a dead connection", result.Failures[0].Err, result.Err)
	}
	if content, _ := server.File("/file"); content != "old" {
		t.Fatalf("target = %q, want the previous content", content)
	}
	if stored := server.CommandCount("STOR"); stored != 1 {
		t.Fatalf("STOR sent %d times, want exactly one attempt", stored)
	}
}

func TestRunDoesNotTouchFailedConnection(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.AddFile("/file", "content")
	server.SetDropCommand(func(command, _ string) bool { return command == "NOOP" })
	host := server.Host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connect(t, host)
	select {
	case <-conn.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("connection monitor did not report failure")
	}
	terminal := conn.Err()
	if terminal == nil {
		t.Fatal("connection closed without a terminal error")
	}
	_, root := openRoot(t)
	result := Run(context.Background(), conn, root, []Item{{RemotePath: "/file", Decision: DecisionDeleteRemote}}, nil)
	if len(result.Completed) != 0 || len(result.Failures) != 0 || !errors.Is(result.Err, terminal) {
		t.Fatalf("result = %+v, want no attempt and the terminal error", result)
	}
	if server.CommandCount("DELE") != 0 {
		t.Fatal("run sent a command on the failed connection")
	}
}

func TestRunStopsWhenCanceled(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.AddFile("/file", "content")
	conn := connect(t, server.Host(t))
	_, root := openRoot(t)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	result := Run(ctx, conn, root, []Item{{RemotePath: "/file", Decision: DecisionDeleteRemote}}, nil)
	if len(result.Completed) != 0 || !errors.Is(result.Err, context.Canceled) {
		t.Fatalf("result = %+v, want cancellation before the first item", result)
	}
	if _, ok := server.File("/file"); !ok {
		t.Fatal("canceled run deleted the file")
	}
}

func TestOperationErrorMarksUncertainOutcome(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.SetDropCommand(func(command, _ string) bool { return command == "NOOP" })
	host := server.Host(t)
	interval := 1
	host.KeepAliveInterval = &interval
	conn := connect(t, host)
	<-conn.Done()
	operationErr := errors.New("delete reply lost")
	err := OperationError(conn, operationErr)
	if !errors.Is(err, operationErr) || !errors.Is(err, conn.Err()) || !strings.Contains(err.Error(), "outcome unknown") {
		t.Fatalf("uncertain error = %v", err)
	}
	if OperationError(conn, nil) != nil {
		t.Fatal("success turned into an error")
	}
}
