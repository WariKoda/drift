package diffview

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	stdsync "sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/remote"
)

// TestLoadDiffItemsUsesSingleFTPSession covers a server that permits only one
// session per user: connecting and browsing succeed, so the diff has to run on
// the connection that is already open instead of failing every file with a
// worker connect error.
func TestLoadDiffItemsUsesSingleFTPSession(t *testing.T) {
	localDir := t.TempDir()
	server := startFTPTestServer(t, 1)
	items := make([]diffLoadItem, 6)
	for i := range items {
		localPath := filepath.Join(localDir, fmt.Sprintf("file%d.txt", i))
		if err := os.WriteFile(localPath, []byte("local\n"), 0o644); err != nil {
			t.Fatalf("write local file: %v", err)
		}
		remotePath := fmt.Sprintf("/file%d.txt", i)
		server.addFile(remotePath, "remote\n")
		items[i] = diffLoadItem{LocalPath: localPath, RemotePath: remotePath, Compare: true}
	}

	host := server.host()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer conn.Close()

	sessions := loadDiffItems(host, conn, items, NewLoadProgressTracker())

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
	if rejected := server.rejectedSessions(); rejected != maxFTPDiffLoadWorkers-1 {
		t.Fatalf("rejected sessions = %d, want %d — extra workers must still try to connect",
			rejected, maxFTPDiffLoadWorkers-1)
	}
}

// TestForEachCompareAddsExtraFTPConnections verifies that reusing the existing
// connection does not collapse the pool to a single worker: when the server
// accepts more sessions, additional workers still connect on their own.
func TestForEachCompareAddsExtraFTPConnections(t *testing.T) {
	server := startFTPTestServer(t, maxFTPDiffLoadWorkers)
	host := server.host()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host)
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
	forEachCompare(host, conn, jobs, nil, func(idx int, workerConn remote.Client) {
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

// ftpTestServer is a real FTP server covering the commands a diff worker sends:
// the login handshake plus SIZE and RETR. It serves at most maxSessions logins
// and answers every further connection with 421, emulating a server that limits
// sessions per user.
type ftpTestServer struct {
	listener    net.Listener
	maxSessions int

	mu       stdsync.Mutex
	files    map[string]string
	accepted int
	rejected int
	wg       stdsync.WaitGroup
}

func startFTPTestServer(t *testing.T, maxSessions int) *ftpTestServer {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	server := &ftpTestServer{
		listener:    listener,
		maxSessions: maxSessions,
		files:       map[string]string{},
	}
	server.wg.Add(1)
	go func() {
		defer server.wg.Done()
		server.acceptLoop()
	}()
	t.Cleanup(func() {
		_ = listener.Close()
		server.wg.Wait()
	})
	return server
}

func (s *ftpTestServer) addFile(remotePath, content string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.files[remotePath] = content
}

func (s *ftpTestServer) file(remotePath string) (string, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	content, ok := s.files[remotePath]
	return content, ok
}

func (s *ftpTestServer) host() config.Host {
	hostname, portString, err := net.SplitHostPort(s.listener.Addr().String())
	if err != nil {
		panic(fmt.Sprintf("split server address: %v", err))
	}
	port, err := strconv.Atoi(portString)
	if err != nil {
		panic(fmt.Sprintf("parse server port: %v", err))
	}
	return config.Host{
		Name:     "test",
		Hostname: hostname,
		Port:     port,
		User:     "drift",
		Auth:     config.Auth{Password: "secret"},
		Protocol: "ftp",
	}
}

func (s *ftpTestServer) rejectedSessions() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.rejected
}

func (s *ftpTestServer) acceptLoop() {
	for {
		conn, err := s.listener.Accept()
		if err != nil {
			return
		}
		allowed := s.reserveSession()
		s.wg.Add(1)
		go func() {
			defer s.wg.Done()
			defer conn.Close()
			if !allowed {
				_, _ = io.WriteString(conn, "421 too many sessions\r\n")
				return
			}
			s.serve(conn)
		}()
	}
}

func (s *ftpTestServer) reserveSession() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.accepted >= s.maxSessions {
		s.rejected++
		return false
	}
	s.accepted++
	return true
}

func (s *ftpTestServer) serve(conn net.Conn) {
	reader := bufio.NewReader(conn)
	writer := bufio.NewWriter(conn)
	reply := func(format string, args ...any) error {
		if _, err := fmt.Fprintf(writer, format+"\r\n", args...); err != nil {
			return err
		}
		return writer.Flush()
	}

	if err := reply("220 drift diff worker test server"); err != nil {
		return
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
			return
		}
		command, argument, _ := strings.Cut(strings.TrimSpace(line), " ")
		switch strings.ToUpper(command) {
		case "USER":
			err = reply("331 password required")
		case "PASS":
			err = reply("230 logged in")
		case "FEAT":
			err = reply("500 features unavailable")
		case "TYPE":
			err = reply("200 transfer type set")
		case "SIZE":
			content, ok := s.file(argument)
			if !ok {
				err = reply("550 file not found")
				break
			}
			err = reply("213 %d", len(content))
		case "EPSV":
			if dataListener != nil {
				_ = dataListener.Close()
			}
			dataListener, err = net.Listen("tcp", "127.0.0.1:0")
			if err == nil {
				err = reply("229 Entering Extended Passive Mode (|||%d|)",
					dataListener.Addr().(*net.TCPAddr).Port)
			}
		case "RETR":
			content, ok := s.file(argument)
			if !ok {
				err = reply("550 file not found")
				break
			}
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
				_, err = io.WriteString(data, content)
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
		case "QUIT":
			_ = reply("221 goodbye")
			return
		default:
			err = reply("502 command not implemented")
		}
		if err != nil {
			return
		}
	}
}
