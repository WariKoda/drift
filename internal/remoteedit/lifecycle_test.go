package remoteedit

import (
	"context"
	"errors"
	"net"
	"os"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestCompletionRetainsBytesChangedAfterUpload(t *testing.T) {
	for _, when := range []string{"after upload", "after review", "after unchanged capture"} {
		t.Run(when, func(t *testing.T) {
			s, conn, server := newFTPSession(t, false, "old")
			if result := s.Prepare(context.Background(), conn); result.Err != nil {
				t.Fatal(result.Err)
			}
			if when != "after unchanged capture" {
				if err := os.WriteFile(s.file, []byte("confirmed"), 0600); err != nil {
					t.Fatal(err)
				}
			}
			if result := s.Capture(context.Background()); result.Err != nil {
				t.Fatal(result.Err)
			}
			switch when {
			case "after upload":
				if result := s.Save(context.Background(), conn, nil); result.Err != nil {
					t.Fatal(result.Err)
				}
			case "after review":
				server.AddFile("/file.txt", "confirmed")
				if result := s.Review(context.Background(), conn); result.Err != nil || !result.Saved {
					t.Fatalf("review: %+v", result)
				}
			}
			if err := os.WriteFile(s.file, []byte("later"), 0600); err != nil {
				t.Fatal(err)
			}
			if err := s.Complete(); err == nil {
				t.Fatal("completion did not report retained later edits")
			}
			if data, err := os.ReadFile(s.file); err != nil || string(data) != "later" {
				t.Fatalf("completion lost later edits: %q, %v", data, err)
			}
		})
	}
}

func TestSnapshotTamperingIsRejectedBeforeTransfer(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("confirmed"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.snapshot, []byte("not confirmed"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Save(context.Background(), conn, nil); result.Err == nil || result.Unknown || result.Saved {
		t.Fatalf("changed snapshot accepted: %+v", result)
	}
	if server.CommandCount("STOR") != 0 {
		t.Fatal("changed snapshot was transferred")
	}
}

func TestReconnectOnlyChecksAndNeedsNewUploadConsent(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("confirmed"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := conn.Close(); err != nil {
		t.Fatal(err)
	}
	newConn, err := s.Reconnect(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = newConn.Close() })
	if !s.TakeConnection(newConn) || s.TakeConnection(newConn) {
		t.Fatal("reconnect ownership was not transferred exactly once")
	}
	if result := s.Review(context.Background(), newConn); result.Err != nil || result.Saved {
		t.Fatalf("review: %+v", result)
	}
	if server.CommandCount("STOR") != 0 {
		t.Fatal("reconnect or review uploaded")
	}
	if result := s.Save(context.Background(), newConn, nil); result.Err != nil || !result.Saved {
		t.Fatalf("confirmed save: %+v", result)
	}
}

func TestShutdownClosesUnacceptedReconnectAndRetainsWorkingCopy(t *testing.T) {
	s, conn, _ := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	newConn, err := s.Reconnect(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Close(false); err != nil {
		t.Fatal(err)
	}
	select {
	case <-newConn.Done():
	default:
		t.Fatal("shutdown leaked a reconnect result")
	}
	if s.TakeConnection(newConn) {
		t.Fatal("closed session accepted a connection")
	}
	if err := s.Complete(); err == nil {
		t.Fatal("completion after shutdown was not rejected")
	}
	if data, err := os.ReadFile(s.file); err != nil || string(data) != "old" {
		t.Fatal("completion after shutdown removed retained data")
	}
}

func TestDownloadLimitAlsoChecksActualStream(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "small advertised size")
	server.SetSendData(func(data net.Conn, _ string) error {
		_, err := data.Write([]byte(strings.Repeat("x", MaxBytes+1)))
		return err
	})
	result := s.Prepare(context.Background(), conn)
	if !errors.Is(result.Err, ErrTooLarge) {
		t.Fatalf("oversized stream = %+v", result)
	}
	if _, err := os.Stat(s.file); !os.IsNotExist(err) {
		t.Fatalf("oversized stream committed a truncated file: %v", err)
	}
}

func TestCancellationInterruptsStalledDownloadAndSerializesShutdown(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	started, release := make(chan struct{}), make(chan struct{})
	unblock := sync.OnceFunc(func() { close(release) })
	t.Cleanup(unblock)
	server.SetSendData(func(data net.Conn, content string) error {
		close(started)
		<-release
		_, err := data.Write([]byte(content))
		return err
	})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	result := make(chan Result, 1)
	go func() { result <- s.Prepare(ctx, conn) }()
	select {
	case <-started:
	case <-time.After(3 * time.Second):
		t.Fatal("download did not start")
	}
	cancel()
	closed := make(chan error, 1)
	go func() { closed <- s.Close(false) }()
	select {
	case message := <-result:
		if message.Err == nil {
			t.Fatal("cancelled download succeeded")
		}
	case <-time.After(3 * time.Second):
		t.Fatal("cancel did not interrupt remote read")
	}
	select {
	case err := <-closed:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("shutdown did not finish")
	}
	unblock()
}
