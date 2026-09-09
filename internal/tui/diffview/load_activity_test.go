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
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tui/loading"
	tea "github.com/charmbracelet/bubbletea"
)

func TestLoadActivityIdleAndCancellationCloseResources(t *testing.T) {
	for _, userCancel := range []bool{false, true} {
		t.Run(map[bool]string{false: "idle", true: "cancel"}[userCancel], func(t *testing.T) {
			parent, cancel := context.WithCancel(context.Background())
			defer cancel()
			a := newLoadActivity(parent, 30*time.Millisecond)
			defer a.cancel(nil)
			defer a.finish(false)
			for range 3 {
				reader, writer := io.Pipe()
				defer writer.Close()
				a.own(reader)
				if userCancel {
					cancel()
				}
				select {
				case <-a.ctx.Done():
				case <-time.After(time.Second):
					t.Fatal("guard did not cancel")
				}
				// Registration after cancellation must close too.
				done := make(chan error, 1)
				go func() { _, err := writer.Write([]byte("x")); done <- err }()
				select {
				case err := <-done:
					if err == nil {
						t.Fatal("resource remained open")
					}
				case <-time.After(time.Second):
					t.Fatal("resource close blocked")
				}
			}
			err := context.Cause(a.ctx)
			if userCancel {
				if !loading.IsCanceled(err) {
					t.Fatalf("cancel cause: %v", err)
				}
			} else if !errors.Is(err, ErrDiffIdleTimeout) || loading.IsCanceled(err) {
				t.Fatalf("idle cause: %v", err)
			}
		})
	}
}

func TestLoadActivitySuccessfulHandoffSurvivesLateCancellation(t *testing.T) {
	for range 50 {
		ctx, cancel := context.WithCancel(context.Background())
		a := newLoadActivity(ctx, 10*time.Millisecond)
		reader, writer := io.Pipe()
		a.own(reader)
		if err := a.finish(true); err != nil {
			t.Fatal(err)
		}
		cancel()
		a.expire() // A queued timer callback must not close the released reader.
		done := make(chan error, 1)
		go func() { _, err := writer.Write([]byte("x")); done <- err }()
		data := make([]byte, 1)
		if _, err := reader.Read(data); err != nil {
			t.Fatalf("late close: %v", err)
		}
		if err := <-done; err != nil {
			t.Fatal(err)
		}
		reader.Close()
		writer.Close()
		a.cancel(nil)
	}
}

func TestLoadCmdActiveTransferOutlivesIdleWindow(t *testing.T) {
	for _, size := range []int{12, 3 * 1024 * 1024} {
		t.Run(map[bool]string{false: "text", true: "hash"}[size > 1024], func(t *testing.T) {
			server := startFTPTestServer(t, 1)
			content := strings.Repeat("x", size)
			server.addFile("/file", content)
			server.mu.Lock()
			server.sendData = func(c net.Conn, data string) error {
				chunk := len(data) / 12
				for len(data) > 0 {
					time.Sleep(25 * time.Millisecond)
					n := min(chunk, len(data))
					if _, err := io.WriteString(c, data[:n]); err != nil {
						return err
					}
					data = data[n:]
				}
				return nil
			}
			server.mu.Unlock()
			root := t.TempDir()
			local := filepath.Join(root, "file")
			if err := os.WriteFile(local, []byte(strings.Repeat("y", size)), 0600); err != nil {
				t.Fatal(err)
			}
			host := server.host(t)
			host.RootPath = "/"
			conn := connectDiffTestHost(t, host)
			tracker := NewLoadProgressTracker()
			start := time.Now()
			msg := loadCmd(17, host, &fs.SelectionState{Marked: map[string]struct{}{local: {}}}, nil, &config.MergedConfig{ProjectRoot: root}, conn, tracker, nil, nil, 150*time.Millisecond)()
			loaded, ok := msg.(MsgDiffLoaded)
			if !ok {
				t.Fatalf("load: %#v", msg)
			}
			defer loaded.Root.Close()
			if time.Since(start) < 300*time.Millisecond || len(loaded.Sessions) != 1 {
				t.Fatalf("missing long comparison: %+v", loaded)
			}
			if session := loaded.Sessions[0]; session.Err != nil || session.Result == nil || !session.Result.HasDiff() {
				t.Fatalf("long comparison did not succeed: %+v", session)
			}
			if loaded.Conn != conn {
				t.Fatal("returned wrapped connection instead of original identity")
			}
			tracker.Cancel()
			time.Sleep(175 * time.Millisecond)
			if _, err := loaded.Conn.Stat("/file"); err != nil {
				t.Fatalf("success connection closed late: %v", err)
			}
		})
	}
}

func TestLoadCmdStalledTransferStopsOnIdleOrCancel(t *testing.T) {
	for _, userCancel := range []bool{false, true} {
		t.Run(map[bool]string{false: "idle", true: "cancel"}[userCancel], func(t *testing.T) {
			server := startFTPTestServer(t, 1)
			server.addFile("/file", "remote")
			started, release := make(chan struct{}), make(chan struct{})
			unblock := sync.OnceFunc(func() { close(release) })
			defer unblock()
			server.mu.Lock()
			server.sendData = func(c net.Conn, data string) error {
				close(started)
				<-release
				_, err := io.WriteString(c, data)
				return err
			}
			server.mu.Unlock()
			root := t.TempDir()
			local := filepath.Join(root, "file")
			host := server.host(t)
			host.RootPath = "/"
			conn := connectDiffTestHost(t, host)
			tracker := NewLoadProgressTracker()
			result := make(chan tea.Msg, 1)
			go func() {
				result <- loadCmd(18, host, nil, &fs.SelectionState{Marked: map[string]struct{}{"/file": {}}}, &config.MergedConfig{ProjectRoot: root}, conn, tracker, nil, nil, 100*time.Millisecond)()
			}()
			select {
			case <-started:
			case <-time.After(time.Second):
				t.Fatal("transfer did not start")
			}
			if userCancel {
				tracker.Cancel()
			}
			select {
			case msg := <-result:
				failure, ok := msg.(MsgDiffError)
				if !ok {
					t.Fatalf("load succeeded: %#v", msg)
				}
				if userCancel {
					if !loading.IsCanceled(failure.Err) {
						t.Fatalf("cancel: %v", failure.Err)
					}
				} else if !errors.Is(failure.Err, ErrDiffIdleTimeout) || loading.IsCanceled(failure.Err) {
					t.Fatalf("idle: %v", failure.Err)
				}
			case <-time.After(2 * time.Second):
				t.Fatal("stalled transfer not interrupted")
			}
			if _, err := os.Stat(local); !errors.Is(err, os.ErrNotExist) {
				t.Fatal("comparison wrote local file")
			}
			unblock()
		})
	}
}

func TestLoadIdleClosesPrimaryAndExtraComparisons(t *testing.T) {
	server := startFTPTestServer(t, 2)
	server.addFile("/file", "payload")
	started, release := make(chan struct{}, 2), make(chan struct{})
	unblock := sync.OnceFunc(func() { close(release) })
	defer unblock()
	server.mu.Lock()
	server.sendData = func(c net.Conn, data string) error {
		started <- struct{}{}
		<-release
		_, err := io.WriteString(c, data)
		return err
	}
	server.mu.Unlock()
	host := server.host(t)
	primary := connectDiffTestHost(t, host)
	a := newLoadActivity(context.Background(), 250*time.Millisecond)
	defer a.cancel(nil)
	defer a.finish(false)
	a.own(primary)
	conn := &loadClient{Client: primary, activity: a}
	done := make(chan error, 1)
	go func() {
		done <- forEachCompare(host, conn, []int{0, 1}, NewLoadProgressTracker(), nil, nil, func(_ int, worker remote.Client) {
			_, _ = worker.ReadFile("/file")
		})
	}()
	for range 2 {
		select {
		case <-started:
		case <-time.After(time.Second):
			t.Fatal("both real transfers must start")
		}
	}
	select {
	case err := <-done:
		if !errors.Is(err, ErrDiffIdleTimeout) {
			t.Fatalf("worker result: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("idle guard did not interrupt all workers")
	}
	unblock()
}

func TestLoadActivityLocalWalkIncludesEmptyDirectoriesAndStops(t *testing.T) {
	root := t.TempDir()
	for _, dir := range []string{"a/b", "c", "node_modules/skip"} {
		if err := os.MkdirAll(filepath.Join(root, dir), 0700); err != nil {
			t.Fatal(err)
		}
	}
	a := newLoadActivity(context.Background(), time.Second)
	defer a.cancel(nil)
	defer a.finish(false)
	before := a.last
	calls := 0
	if err := a.walkLocal(root, func(string) error { calls++; return nil }); err != nil {
		t.Fatal(err)
	}
	if calls != 0 || !a.last.After(before) {
		t.Fatal("empty directories did not count as activity")
	}
	a.cancel(context.Canceled)
	if err := a.walkLocal(root, func(string) error { t.Fatal("callback after cancellation"); return nil }); !errors.Is(err, context.Canceled) {
		t.Fatalf("walk cancellation: %v", err)
	}
}
