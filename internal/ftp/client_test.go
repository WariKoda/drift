package ftp

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
)

func TestClientSerializesStreamingReadAndStat(t *testing.T) {
	server := startProtocolTestServer(t)
	host, portString, err := net.SplitHostPort(server.Addr().String())
	if err != nil {
		t.Fatalf("split server address: %v", err)
	}
	port, err := strconv.Atoi(portString)
	if err != nil {
		t.Fatalf("parse server port: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	client, err := Connect(ctx, config.Host{
		Name:     "test",
		Hostname: host,
		Port:     port,
		User:     "drift",
		Auth:     config.Auth{Password: "secret"},
		Protocol: "ftp",
	})
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer client.Close()

	reader, err := client.Open("/file.txt")
	if err != nil {
		t.Fatalf("open remote file: %v", err)
	}
	content, err := io.ReadAll(reader)
	if err != nil {
		t.Fatalf("read remote file: %v", err)
	}
	if string(content) != "payload" {
		t.Fatalf("content = %q, want %q", content, "payload")
	}

	statDone := make(chan error, 1)
	go func() {
		_, statErr := client.Stat("/file.txt")
		statDone <- statErr
	}()

	select {
	case <-server.sizeSeen:
		t.Fatal("SIZE reached the server before the active RETR stream was closed")
	case err := <-statDone:
		t.Fatalf("Stat returned before the active RETR stream was closed: %v", err)
	case <-time.After(100 * time.Millisecond):
	}

	if err := reader.Close(); err != nil {
		t.Fatalf("close remote file: %v", err)
	}
	// Closing twice must neither unlock the client twice nor close the
	// underlying FTP response a second time.
	if err := reader.Close(); err != nil {
		t.Fatalf("close remote file again: %v", err)
	}

	select {
	case err := <-statDone:
		if err != nil {
			t.Fatalf("stat after close: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Stat remained blocked after the RETR stream was closed")
	}
}

type protocolTestServer struct {
	net.Listener
	sizeSeen chan struct{}
	done     chan error
}

func startProtocolTestServer(t *testing.T) *protocolTestServer {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	server := &protocolTestServer{
		Listener: listener,
		sizeSeen: make(chan struct{}, 1),
		done:     make(chan error, 1),
	}
	go func() {
		server.done <- server.serve()
	}()
	t.Cleanup(func() {
		_ = server.Listener.Close()
		select {
		case err := <-server.done:
			if err != nil && !errors.Is(err, net.ErrClosed) {
				t.Errorf("FTP server: %v", err)
			}
		case <-time.After(2 * time.Second):
			t.Error("FTP server did not stop")
		}
	})
	return server
}

func (s *protocolTestServer) serve() error {
	control, err := s.Accept()
	if err != nil {
		return err
	}
	defer control.Close()

	reader := bufio.NewReader(control)
	writer := bufio.NewWriter(control)
	reply := func(format string, args ...any) error {
		if _, err := fmt.Fprintf(writer, format+"\r\n", args...); err != nil {
			return err
		}
		return writer.Flush()
	}

	if err := reply("220 drift protocol test server"); err != nil {
		return err
	}

	var dataListener net.Listener
	defer func() {
		if dataListener != nil {
			_ = dataListener.Close()
		}
	}()

	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			return err
		}
		line = strings.TrimSpace(line)
		command, _, _ := strings.Cut(line, " ")
		switch strings.ToUpper(command) {
		case "USER":
			err = reply("331 password required")
		case "PASS":
			err = reply("230 logged in")
		case "FEAT":
			err = reply("500 features unavailable")
		case "TYPE":
			err = reply("200 transfer type set")
		case "EPSV":
			if dataListener != nil {
				_ = dataListener.Close()
			}
			dataListener, err = net.Listen("tcp", "127.0.0.1:0")
			if err == nil {
				port := dataListener.Addr().(*net.TCPAddr).Port
				err = reply("229 Entering Extended Passive Mode (|||%d|)", port)
			}
		case "RETR":
			if dataListener == nil {
				err = reply("425 use EPSV first")
				break
			}
			var data net.Conn
			data, err = dataListener.Accept()
			if err != nil {
				break
			}
			if err = reply("150 opening data connection"); err == nil {
				_, err = io.WriteString(data, "payload")
			}
			closeErr := data.Close()
			if err == nil {
				err = closeErr
			}
			if err == nil {
				err = reply("226 transfer complete")
			}
			_ = dataListener.Close()
			dataListener = nil
		case "SIZE":
			select {
			case s.sizeSeen <- struct{}{}:
			default:
			}
			err = reply("213 7")
		case "QUIT":
			if err := reply("221 goodbye"); err != nil {
				return err
			}
			return nil
		default:
			err = reply("502 command not implemented")
		}
		if err != nil {
			return err
		}
	}
}

// TestWalkQueueSurvivesWideFanOut walks a tree with far more directories than
// any bounded queue would hold. The previous walker shared one 4096-slot
// channel between readers and writers, so a tree this wide filled the buffer,
// every worker blocked on its own send, and the walk never finished.
func TestWalkQueueSurvivesWideFanOut(t *testing.T) {
	const (
		branches = 200
		leaves   = 50 // 1 + 200 + 10000 directories, well past any 4096 buffer
	)
	tree := map[string][]string{}
	for b := range branches {
		branch := fmt.Sprintf("/b%d", b)
		tree["/"] = append(tree["/"], branch)
		for l := range leaves {
			tree[branch] = append(tree[branch], fmt.Sprintf("%s/l%d", branch, l))
		}
	}

	var mu sync.Mutex
	listed := map[string]int{}
	listers := make([]func(string) []string, 0, maxWalkWorkers)
	for range maxWalkWorkers {
		listers = append(listers, func(dir string) []string {
			mu.Lock()
			defer mu.Unlock()
			listed[dir]++
			return tree[dir]
		})
	}

	done := make(chan struct{})
	go func() {
		walkQueue("/", listers, func() bool { return false })
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(30 * time.Second):
		t.Fatal("walk did not finish: the directory queue deadlocked")
	}

	mu.Lock()
	defer mu.Unlock()
	if len(listed) != len(tree)+branches*leaves {
		t.Errorf("listed %d directories, want %d", len(listed), len(tree)+branches*leaves)
	}
	for dir, count := range listed {
		if count != 1 {
			t.Fatalf("directory %s listed %d times, want once", dir, count)
		}
	}
}

// TestWalkQueueStopsWhenCallerGivesUp checks that a failure recorded by the
// first listing keeps the remaining directories from being visited.
func TestWalkQueueStopsWhenCallerGivesUp(t *testing.T) {
	children := make([]string, 1000)
	for i := range children {
		children[i] = fmt.Sprintf("/c%d", i)
	}

	var mu sync.Mutex
	var listed []string
	lister := func(dir string) []string {
		mu.Lock()
		defer mu.Unlock()
		listed = append(listed, dir)
		if dir == "/" {
			return children
		}
		return nil
	}
	stop := func() bool {
		mu.Lock()
		defer mu.Unlock()
		return len(listed) > 0
	}

	listers := make([]func(string) []string, 0, maxWalkWorkers)
	for range maxWalkWorkers {
		listers = append(listers, lister)
	}
	walkQueue("/", listers, stop)

	mu.Lock()
	defer mu.Unlock()
	if len(listed) != 1 {
		t.Errorf("listed %d directories, want only the root", len(listed))
	}
}

// TestWalkQueueVisitsEveryDirectoryWithOneLister covers the degenerate case:
// when no extra connection could be opened, the single client both hands out
// and consumes the queue.
func TestWalkQueueVisitsEveryDirectoryWithOneLister(t *testing.T) {
	tree := map[string][]string{
		"/":       {"/a", "/b"},
		"/a":      {"/a/deep"},
		"/a/deep": {"/a/deep/deeper"},
	}
	var listed []string
	walkQueue("/", []func(string) []string{
		func(dir string) []string {
			listed = append(listed, dir)
			return tree[dir]
		},
	}, func() bool { return false })

	want := []string{"/", "/a", "/b", "/a/deep", "/a/deep/deeper"}
	if strings.Join(listed, " ") != strings.Join(want, " ") {
		t.Errorf("listed %v, want %v", listed, want)
	}
}
