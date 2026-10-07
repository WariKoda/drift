//go:build unix

package ssh

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"errors"
	"io"
	"net"
	"os"
	"path/filepath"
	"testing"
	"time"

	"golang.org/x/crypto/ssh/agent"

	"github.com/WariKoda/drift/internal/config"
)

func TestAuthMethodsAgentCloserClosesSocket(t *testing.T) {
	for _, tc := range []struct {
		name string
		auth config.Auth
	}{
		{"agent", config.Auth{Type: "agent"}},
		{"empty-keyfile", config.Auth{Type: "keyfile"}},
		{"expanded-empty-keyfile", config.Auth{Type: "keyfile", KeyFile: "$DRIFT_TEST_KEYFILE"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			// Unix socket paths have a small limit, especially on macOS.
			dir, err := os.MkdirTemp("", "drift-agent-")
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() {
				if err := os.RemoveAll(dir); err != nil {
					t.Error(err)
				}
			})
			sock := filepath.Join(dir, "agent.sock")
			listener, err := net.Listen("unix", sock)
			if err != nil {
				t.Skipf("unix socket unavailable: %v", err)
			}
			defer listener.Close()

			_, key, err := ed25519.GenerateKey(rand.Reader)
			if err != nil {
				t.Fatal(err)
			}
			keyring := agent.NewKeyring()
			if err := keyring.Add(agent.AddedKey{PrivateKey: key}); err != nil {
				t.Fatal(err)
			}
			accepted := make(chan net.Conn, 1)
			agentDone := make(chan error, 1)
			go func() {
				conn, err := listener.Accept()
				if err != nil {
					agentDone <- err
					return
				}
				defer conn.Close()
				accepted <- conn
				agentDone <- agent.ServeAgent(keyring, conn)
			}()

			t.Setenv("SSH_AUTH_SOCK", sock)
			t.Setenv("DRIFT_TEST_KEYFILE", "")
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			methods, closer, err := AuthMethods(ctx, tc.auth)
			if err != nil {
				t.Fatal(err)
			}
			if closer != nil {
				defer closer.Close()
			}
			select {
			case conn := <-accepted:
				defer conn.Close()
			case err := <-agentDone:
				t.Fatalf("agent accept: %v", err)
			case <-ctx.Done():
				t.Fatal("agent did not accept the socket")
			}
			if len(methods) != 1 {
				t.Fatalf("auth methods = %d, want 1", len(methods))
			}
			conn, ok := closer.(net.Conn)
			if !ok {
				t.Fatalf("agent closer is %T, want a net.Conn", closer)
			}
			signers, err := agent.NewClient(conn).Signers()
			if err != nil || len(signers) != 1 {
				t.Fatalf("agent signers = %v, err = %v", signers, err)
			}
			data := []byte("agent cleanup regression")
			signature, err := signers[0].Sign(rand.Reader, data)
			if err != nil {
				t.Fatal(err)
			}
			if err := signers[0].PublicKey().Verify(data, signature); err != nil {
				t.Fatalf("agent signature: %v", err)
			}

			if err := closer.Close(); err != nil {
				t.Fatal(err)
			}
			select {
			case err := <-agentDone:
				if !errors.Is(err, io.EOF) {
					t.Fatalf("agent shutdown = %v, want EOF after closing the client socket", err)
				}
			case <-ctx.Done():
				t.Fatal("closing the auth closer did not disconnect the agent")
			}
		})
	}
}
