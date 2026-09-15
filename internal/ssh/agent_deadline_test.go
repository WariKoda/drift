//go:build unix

package ssh

import (
	"context"
	"net"
	"path/filepath"
	"testing"
	"time"

	"golang.org/x/crypto/ssh/agent"

	"github.com/WariKoda/drift/internal/config"
)

// A real agent socket that accepts the connection and then stays silent, the
// way a smartcard agent waiting for a PIN does. Without a deadline on the
// socket the first request never returns.
func TestAgentRequestsHonourTheConnectDeadline(t *testing.T) {
	sock := filepath.Join(t.TempDir(), "agent.sock")
	listener, err := net.Listen("unix", sock)
	if err != nil {
		t.Skipf("unix socket unavailable: %v", err)
	}
	defer listener.Close()
	go func() {
		for {
			conn, err := listener.Accept()
			if err != nil {
				return
			}
			// Hold the connection open and answer nothing.
			defer conn.Close()
		}
	}()

	t.Setenv("SSH_AUTH_SOCK", sock)
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()

	_, closer, err := AuthMethods(ctx, config.Auth{Type: "agent"})
	if err != nil {
		t.Fatalf("AuthMethods returned error: %v", err)
	}
	defer closer.Close()

	conn, ok := closer.(net.Conn)
	if !ok {
		t.Fatalf("agent closer is %T, want a net.Conn", closer)
	}

	done := make(chan error, 1)
	go func() {
		_, err := agent.NewClient(conn).Signers()
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("Signers succeeded against an agent that never answered")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Signers blocked although the context deadline had passed")
	}
}
