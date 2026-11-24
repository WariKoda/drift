// Package sshtest provides a real in-process SSH server with an SFTP subsystem
// for tests that need a remote host without external infrastructure.
package sshtest

import (
	"crypto/ed25519"
	"crypto/rand"
	"net"
	"sync"
	"sync/atomic"
	"testing"

	pkgsftp "github.com/pkg/sftp"
	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/ssh"
)

// Server accepts any number of SSH connections with password authentication
// and serves the local directory root over SFTP. Its host key is new for every
// server, so no known_hosts file lists it.
type Server struct {
	listener  net.Listener
	root      string
	hostKey   gossh.PublicKey
	authTries atomic.Int32

	mu    sync.Mutex
	conns map[net.Conn]struct{}
	wg    sync.WaitGroup
}

// Start runs the server until the test ends.
func Start(t testing.TB, root string) *Server {
	t.Helper()
	_, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatalf("generate host key: %v", err)
	}
	signer, err := gossh.NewSignerFromKey(private)
	if err != nil {
		t.Fatalf("host key signer: %v", err)
	}
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	server := &Server{listener: listener, root: root, hostKey: signer.PublicKey(), conns: map[net.Conn]struct{}{}}
	cfg := &gossh.ServerConfig{
		PasswordCallback: func(gossh.ConnMetadata, []byte) (*gossh.Permissions, error) {
			server.authTries.Add(1)
			return nil, nil
		},
	}
	cfg.AddHostKey(signer)
	server.wg.Add(1)
	go func() {
		defer server.wg.Done()
		server.acceptLoop(cfg)
	}()
	t.Cleanup(func() {
		_ = listener.Close()
		server.mu.Lock()
		for conn := range server.conns {
			_ = conn.Close()
		}
		server.mu.Unlock()
		server.wg.Wait()
	})
	return server
}

// Host returns a password-authenticated SFTP host for this server. Keep-alive
// probes are off.
func (s *Server) Host() config.Host {
	zero := 0
	return config.Host{
		Name:              "ssh",
		Hostname:          "127.0.0.1",
		Port:              s.listener.Addr().(*net.TCPAddr).Port,
		User:              "test",
		RootPath:          s.root,
		Auth:              config.Auth{Type: "password", Password: "secret"},
		KeepAliveInterval: &zero,
	}
}

// Challenge returns what a trust prompt for this server shows.
func (s *Server) Challenge() ssh.HostKeyChallenge {
	return ssh.HostKeyChallenge{Address: knownhosts.Normalize(s.listener.Addr().String()), Key: s.hostKey}
}

// AuthAttempts counts the passwords the server has received.
func (s *Server) AuthAttempts() int {
	return int(s.authTries.Load())
}

func (s *Server) acceptLoop(cfg *gossh.ServerConfig) {
	for {
		socket, err := s.listener.Accept()
		if err != nil {
			return
		}
		s.mu.Lock()
		s.conns[socket] = struct{}{}
		s.mu.Unlock()
		s.wg.Add(1)
		go func() {
			defer s.wg.Done()
			defer func() {
				_ = socket.Close()
				s.mu.Lock()
				delete(s.conns, socket)
				s.mu.Unlock()
			}()
			s.serve(socket, cfg)
		}()
	}
}

func (s *Server) serve(socket net.Conn, cfg *gossh.ServerConfig) {
	conn, channels, requests, err := gossh.NewServerConn(socket, cfg)
	if err != nil {
		return // the client rejected the host key, or the test ended
	}
	defer conn.Close()
	go gossh.DiscardRequests(requests)
	for channel := range channels {
		if channel.ChannelType() != "session" {
			_ = channel.Reject(gossh.UnknownChannelType, "session required")
			continue
		}
		stream, sessionRequests, err := channel.Accept()
		if err != nil {
			continue
		}
		for request := range sessionRequests {
			var subsystem struct{ Name string }
			ok := request.Type == "subsystem" && gossh.Unmarshal(request.Payload, &subsystem) == nil && subsystem.Name == "sftp"
			if err := request.Reply(ok, nil); err != nil {
				break
			}
			if !ok {
				continue
			}
			server, err := pkgsftp.NewServer(stream, pkgsftp.WithServerWorkingDirectory(s.root))
			if err == nil {
				_ = server.Serve()
				_ = server.Close()
			}
			break
		}
		_ = stream.Close()
	}
}
