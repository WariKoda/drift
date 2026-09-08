package sftp

import (
	"bufio"
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	pkgsftp "github.com/pkg/sftp"
	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/agent"

	"github.com/WariKoda/drift/internal/config"
)

const testKeepAliveInterval = 10 * time.Millisecond

// localSSHServer serves actual SSH global requests and an SFTP filesystem over
// loopback. Tests decide when (and whether) to reply to each global request.
type localSSHServer struct {
	host       config.Host
	root       string
	requests   chan *gossh.Request
	listener   net.Listener
	transport  net.Conn // published by closing accepted
	accepted   chan struct{}
	done       chan struct{}
	stop       chan struct{}
	rejectSFTP bool
}

func newLocalSSHServer(t *testing.T, rejectSFTP bool) *localSSHServer {
	t.Helper()
	t.Setenv("HOME", t.TempDir()) // isolate known_hosts
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(key)
	if err != nil {
		t.Fatal(err)
	}
	cfg := &gossh.ServerConfig{
		PasswordCallback: func(_ gossh.ConnMetadata, password []byte) (*gossh.Permissions, error) {
			if string(password) != "test-password" {
				return nil, fmt.Errorf("invalid password")
			}
			return nil, nil
		},
	}
	cfg.AddHostKey(signer)
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	s := &localSSHServer{
		host: config.Host{
			Name: "keepalive-test", Hostname: "127.0.0.1",
			Port: listener.Addr().(*net.TCPAddr).Port, User: "test",
			Auth: config.Auth{Type: "password", Password: "test-password"},
		},
		root: t.TempDir(), requests: make(chan *gossh.Request, 16),
		listener: listener, accepted: make(chan struct{}), done: make(chan struct{}),
		stop: make(chan struct{}), rejectSFTP: rejectSFTP,
	}
	go s.serve(t, cfg)
	t.Cleanup(func() {
		close(s.stop)
		_ = listener.Close()
		<-s.accepted
		if s.transport != nil {
			_ = s.transport.Close()
		}
		waitSignal(t, s.done, "SSH server shutdown")
	})
	return s
}

func (s *localSSHServer) serve(t *testing.T, cfg *gossh.ServerConfig) {
	defer close(s.done)
	transport, err := s.listener.Accept()
	s.transport = transport
	close(s.accepted)
	if err != nil {
		return
	}
	defer transport.Close()
	conn, channels, requests, err := gossh.NewServerConn(transport, cfg)
	if err != nil {
		return // authentication failure and cancellation are tested too
	}
	defer conn.Close()
	var workers sync.WaitGroup
	workers.Add(1)
	go func() {
		defer workers.Done()
		for request := range requests {
			select {
			case s.requests <- request:
			case <-s.stop:
				return
			}
		}
	}()
	for channel := range channels {
		if channel.ChannelType() != "session" {
			_ = channel.Reject(gossh.UnknownChannelType, "only sessions supported")
			continue
		}
		ch, reqs, err := channel.Accept()
		if err != nil {
			continue
		}
		workers.Add(1)
		go func() {
			defer workers.Done()
			defer ch.Close()
			for request := range reqs {
				var subsystem struct{ Name string }
				ok := request.Type == "subsystem" &&
					gossh.Unmarshal(request.Payload, &subsystem) == nil &&
					subsystem.Name == "sftp" && !s.rejectSFTP
				if err := request.Reply(ok, nil); err != nil {
					return
				}
				if !ok {
					continue
				}
				server, err := pkgsftp.NewServer(ch, pkgsftp.WithServerWorkingDirectory(s.root))
				if err != nil {
					t.Errorf("create SFTP server: %v", err)
					return
				}
				_ = server.Serve()
				_ = server.Close()
				return
			}
		}()
	}
	workers.Wait()
}

func waitSignal(t *testing.T, signal <-chan struct{}, description string) {
	t.Helper()
	select {
	case <-signal:
	case <-time.After(5 * time.Second):
		t.Fatalf("timed out waiting for %s", description)
	}
}

func nextKeepAlive(t *testing.T, server *localSSHServer) *gossh.Request {
	t.Helper()
	select {
	case request := <-server.requests:
		if request.Type != "keepalive@openssh.com" || !request.WantReply || len(request.Payload) != 0 {
			t.Fatalf("unexpected global request: %#v", request)
		}
		return request
	case <-time.After(5 * time.Second):
		t.Fatal("no SSH keepalive received")
		return nil
	}
}

func connectLocal(t *testing.T, server *localSSHServer, interval, timeout time.Duration) *Client {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel() // successful client must outlive this context
	client, err := connect(ctx, server.host, interval, timeout)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		closed := make(chan struct{})
		go func() {
			defer close(closed)
			if err := client.Close(); err != nil {
				t.Errorf("Close: %v", err)
			}
		}()
		waitSignal(t, closed, "client Close")
	})
	return client
}

func TestKeepAliveRepliesAndParallelRead(t *testing.T) {
	for _, reply := range []bool{true, false} {
		t.Run(fmt.Sprintf("reply=%t", reply), func(t *testing.T) {
			server := newLocalSSHServer(t, false)
			content := bytes.Repeat([]byte("SFTP data during SSH probe\n"), 65536)
			if err := os.WriteFile(filepath.Join(server.root, "data"), content, 0o600); err != nil {
				t.Fatal(err)
			}
			client := connectLocal(t, server, testKeepAliveInterval, time.Second)
			request := nextKeepAlive(t, server)
			// Leave the probe awaiting its reply while a real SFTP download runs.
			readDone := make(chan error, 1)
			go func() {
				data, err := client.ReadFile("data")
				if err == nil && !bytes.Equal(data, content) {
					err = fmt.Errorf("downloaded content differs")
				}
				readDone <- err
			}()
			select {
			case err := <-readDone:
				if err != nil {
					t.Fatal(err)
				}
			case <-time.After(3 * time.Second):
				t.Fatal("SFTP read blocked behind SSH probe")
			}
			if err := request.Reply(reply, nil); err != nil {
				t.Fatal(err)
			}
			// A second probe confirms the previous reply was accepted, including
			// a negative reply, and that canceling the connect context did not stop it.
			request = nextKeepAlive(t, server)
			if err := request.Reply(reply, nil); err != nil {
				t.Fatal(err)
			}
			select {
			case <-client.Done():
				t.Fatalf("healthy peer disconnected: %v", client.Err())
			default:
			}
			if err := client.Close(); err != nil || client.Err() != nil {
				t.Fatalf("normal Close: %v, terminal error: %v", err, client.Err())
			}
		})
	}
}

func TestKeepAliveTimeout(t *testing.T) {
	server := newLocalSSHServer(t, false)
	client := connectLocal(t, server, testKeepAliveInterval, 80*time.Millisecond)
	_ = nextKeepAlive(t, server) // deliberately never reply
	done := client.Done()
	waitSignal(t, done, "probe timeout")
	terminal := client.Err()
	if terminal == nil || !strings.Contains(terminal.Error(), "keepalive timed out") {
		t.Fatalf("terminal error = %v", terminal)
	}
	waitSignal(t, server.done, "timeout transport closure")
	if _, err := client.ReadDir("."); err == nil {
		t.Fatal("operation succeeded on failed connection")
	}
	if err := client.Close(); err != nil {
		t.Fatal(err)
	}
	if client.Done() != done || client.Err() != terminal {
		t.Fatal("Close replaced terminal signal or error")
	}
	select {
	case <-server.requests:
		t.Fatal("more than one unanswered probe was sent")
	default:
	}
}

func TestKeepAliveDisabled(t *testing.T) {
	server := newLocalSSHServer(t, false)
	zero := 0
	server.host.KeepAliveInterval = &zero
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	client, err := Connect(ctx, server.host)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	select {
	case <-server.requests:
		t.Fatal("probe sent with keepalive disabled")
	case <-time.After(5 * testKeepAliveInterval):
	}
	if _, err := client.ReadDir("."); err != nil {
		t.Fatalf("canceling connect context closed client: %v", err)
	}
	if err := client.Close(); err != nil || client.Err() != nil {
		t.Fatalf("normal Close: %v, terminal error: %v", err, client.Err())
	}
	waitSignal(t, client.Done(), "disabled client shutdown")
}

func TestKeepAliveTransportFailure(t *testing.T) {
	for _, interval := range []time.Duration{0, testKeepAliveInterval} {
		t.Run(interval.String(), func(t *testing.T) {
			server := newLocalSSHServer(t, false)
			client := connectLocal(t, server, interval, time.Second)
			if interval > 0 {
				_ = nextKeepAlive(t, server)
			}
			<-server.accepted
			_ = server.transport.Close()
			waitSignal(t, client.Done(), "transport failure")
			if client.Err() == nil {
				t.Fatal("transport failure has no terminal error")
			}
			terminal := client.Err()
			_ = client.Close()
			if client.Err() != terminal {
				t.Fatal("Close replaced transport error")
			}
		})
	}
}

func TestKeepAliveConcurrentCloseDuringProbe(t *testing.T) {
	server := newLocalSSHServer(t, false)
	client := connectLocal(t, server, testKeepAliveInterval, time.Hour)
	_ = nextKeepAlive(t, server)
	var callers sync.WaitGroup
	for range 20 {
		callers.Add(1)
		go func() {
			defer callers.Done()
			if err := client.Close(); err != nil {
				t.Errorf("Close: %v", err)
			}
			if client.Err() != nil {
				t.Errorf("normal Close reported failure: %v", client.Err())
			}
		}()
	}
	closed := make(chan struct{})
	go func() {
		callers.Wait()
		close(closed)
	}()
	waitSignal(t, closed, "concurrent Close during unanswered probe")
	waitSignal(t, client.Done(), "monitor cleanup")
	waitSignal(t, server.done, "server disconnection")
}

func TestKeepAliveFailedSFTPStartup(t *testing.T) {
	server := newLocalSSHServer(t, true)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	client, err := connect(ctx, server.host, time.Nanosecond, time.Second)
	if err == nil || client != nil {
		t.Fatalf("rejected subsystem: client=%v error=%v", client, err)
	}
	waitSignal(t, server.done, "failed setup cleanup")
	select {
	case <-server.requests:
		t.Fatal("monitor started before successful SFTP startup")
	default:
	}
}

func TestConnectFailureClosesAuthResources(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	// Fail known_hosts setup after a real SSH-agent socket has been opened.
	if err := os.WriteFile(filepath.Join(os.Getenv("HOME"), ".ssh"), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	socket := filepath.Join(t.TempDir(), "agent.sock")
	listener, err := net.Listen("unix", socket)
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	t.Setenv("SSH_AUTH_SOCK", socket)
	agentDone := make(chan struct{})
	go func() {
		defer close(agentDone)
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		defer conn.Close()
		_ = agent.ServeAgent(agent.NewKeyring(), conn)
	}()
	client, err := Connect(context.Background(), config.Host{
		Hostname: "127.0.0.1", Auth: config.Auth{Type: "agent"},
	})
	if err == nil || client != nil || !strings.Contains(err.Error(), "known_hosts") {
		t.Fatalf("known_hosts failure: client=%v error=%v", client, err)
	}
	waitSignal(t, agentDone, "auth socket cleanup after failed setup")
}

func TestConnectCancellationDuringSSHHandshake(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	handshake := make(chan struct{})
	serverDone := make(chan struct{})
	go func() {
		defer close(serverDone)
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		defer conn.Close()
		reader := bufio.NewReader(conn)
		if _, err := reader.ReadString('\n'); err != nil {
			return
		}
		close(handshake)
		// Never send a server identification, leaving SSH setup blocked.
		_, _ = io.Copy(io.Discard, reader)
	}()
	result := make(chan error, 1)
	go func() {
		client, err := Connect(ctx, config.Host{
			Hostname: "127.0.0.1", Port: listener.Addr().(*net.TCPAddr).Port,
			Auth: config.Auth{Type: "password", Password: "unused"},
		})
		if client != nil {
			_ = client.Close()
		}
		result <- err
	}()
	waitSignal(t, handshake, "SSH identification")
	cancel()
	select {
	case err := <-result:
		if err == nil {
			t.Fatal("canceled SSH handshake succeeded")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("canceled SSH handshake did not exit")
	}
	waitSignal(t, serverDone, "canceled handshake transport closure")
}

func TestKeepAliveShortLivedClients(t *testing.T) {
	for i := range 8 {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			server := newLocalSSHServer(t, false)
			client := connectLocal(t, server, time.Nanosecond, time.Hour)
			done := client.Done()
			if err := client.Close(); err != nil {
				t.Fatal(err)
			}
			waitSignal(t, done, "short-lived monitor shutdown")
			waitSignal(t, server.done, "short-lived SSH shutdown")
			if client.Err() != nil || client.Done() != done {
				t.Fatalf("unstable terminal state: %v", client.Err())
			}
		})
	}
}
