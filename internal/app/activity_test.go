package app

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
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
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
				if !progress.IsCanceled(err) {
					t.Fatalf("cancel cause: %v", err)
				}
			} else if !errors.Is(err, ErrIdleTimeout) || progress.IsCanceled(err) {
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
			server := ftptest.Start(t, 1)
			content := strings.Repeat("x", size)
			server.AddFile("/file", content)
			server.SetSendData(func(c net.Conn, data string) error {
				chunk := len(data) / 12
				for len(data) > 0 {
					time.Sleep(100 * time.Millisecond)
					n := min(chunk, len(data))
					if _, err := io.WriteString(c, data[:n]); err != nil {
						return err
					}
					data = data[n:]
				}
				return nil
			})
			root := t.TempDir()
			local := filepath.Join(root, "file")
			if err := os.WriteFile(local, []byte(strings.Repeat("y", size)), 0600); err != nil {
				t.Fatal(err)
			}
			host := server.Host(t)
			host.RootPath = "/"
			conn := connectTestHost(t, host)
			// The window has to cover the Git processes that classify the
			// selection before any byte moves; a busy CI runner needed more than
			// 150ms for them. The transfer itself still takes three windows.
			const idle = 400 * time.Millisecond
			tracker := progress.NewTracker("Connecting…")
			start := time.Now()
			loaded, err := Load(tracker.Context(), LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: root},
				Local: &fs.SelectionState{Marked: map[string]struct{}{local: {}}}, Conn: conn, IdleTimeout: idle}, tracker)
			if err != nil {
				t.Fatalf("load: %v", err)
			}
			defer loaded.Root.Close()
			if time.Since(start) < 2*idle || len(loaded.Sessions) != 1 {
				t.Fatalf("missing long comparison: %+v", loaded)
			}
			if session := loaded.Sessions[0]; session.Err != nil || session.Result == nil || !session.Result.HasDiff() {
				t.Fatalf("long comparison did not succeed: %+v", session)
			}
			if loaded.Conn != conn {
				t.Fatal("returned wrapped connection instead of original identity")
			}
			tracker.Cancel()
			time.Sleep(idle + 50*time.Millisecond)
			if _, err := loaded.Conn.Stat("/file"); err != nil {
				t.Fatalf("success connection closed late: %v", err)
			}
		})
	}
}

func TestLoadCmdStalledTransferStopsOnIdleOrCancel(t *testing.T) {
	for _, userCancel := range []bool{false, true} {
		t.Run(map[bool]string{false: "idle", true: "cancel"}[userCancel], func(t *testing.T) {
			server := ftptest.Start(t, 1)
			server.AddFile("/file", "remote")
			started, release := make(chan struct{}), make(chan struct{})
			unblock := sync.OnceFunc(func() { close(release) })
			defer unblock()
			server.SetSendData(func(c net.Conn, data string) error {
				close(started)
				<-release
				_, err := io.WriteString(c, data)
				return err
			})
			root := t.TempDir()
			local := filepath.Join(root, "file")
			host := server.Host(t)
			host.RootPath = "/"
			conn := connectTestHost(t, host)
			tracker := progress.NewTracker("Connecting…")
			result := make(chan error, 1)
			go func() {
				_, err := Load(tracker.Context(), LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: root},
					Remote: &fs.SelectionState{Marked: map[string]struct{}{"/file": {}}}, Conn: conn, IdleTimeout: 100 * time.Millisecond}, tracker)
				result <- err
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
			case err := <-result:
				if err == nil {
					t.Fatal("load succeeded")
				}
				if userCancel {
					if !progress.IsCanceled(err) {
						t.Fatalf("cancel: %v", err)
					}
				} else if !errors.Is(err, ErrIdleTimeout) || progress.IsCanceled(err) {
					t.Fatalf("idle: %v", err)
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
	server := ftptest.Start(t, 2)
	server.AddFile("/file", "payload")
	started, release := make(chan struct{}, 2), make(chan struct{})
	unblock := sync.OnceFunc(func() { close(release) })
	defer unblock()
	server.SetSendData(func(c net.Conn, data string) error {
		started <- struct{}{}
		<-release
		_, err := io.WriteString(c, data)
		return err
	})
	host := server.Host(t)
	primary := connectTestHost(t, host)
	a := newLoadActivity(context.Background(), 250*time.Millisecond)
	defer a.cancel(nil)
	defer a.finish(false)
	a.own(primary)
	conn := &loadClient{Client: primary, activity: a}
	done := make(chan error, 1)
	go func() {
		done <- forEachCompare(a.ctx, host, conn, []int{0, 1}, progress.NewTracker("Connecting…"), nil, nil, func(_ int, worker remote.Client) {
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
		if !errors.Is(err, ErrIdleTimeout) {
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
