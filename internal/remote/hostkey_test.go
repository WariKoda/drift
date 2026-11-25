package remote

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/ssh"
	"github.com/WariKoda/drift/internal/sshtest"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// Every caller reaches SFTP through Connect with the root-owned trust manager,
// so this is the path a first connection takes in the application.
func TestConnectAsksBeforeTrustingAnSSHHostKey(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	remoteDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(remoteDir, "file.txt"), []byte("content"), 0o600); err != nil {
		t.Fatal(err)
	}
	server := sshtest.Start(t, remoteDir)
	host := server.Host()
	trust := tlstrust.NewManager()
	connect := func(manager *tlstrust.Manager) (Client, error) {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		return Connect(ctx, host, manager, nil)
	}

	for name, manager := range map[string]*tlstrust.Manager{"without a manager": nil, "before approval": trust} {
		conn, err := connect(manager)
		var unknown *ssh.UnknownHostKeyError
		if conn != nil || !errors.As(err, &unknown) {
			t.Fatalf("%s: conn = %v, error = %v; want an unknown host key error", name, conn, err)
		}
		if !unknown.Challenge.Equal(server.Challenge()) {
			t.Fatalf("%s: challenge names %s %s", name, unknown.Challenge.Address, unknown.Challenge.Fingerprint())
		}
	}
	if tries := server.AuthAttempts(); tries != 0 {
		t.Fatalf("server received %d password(s) before the key was approved", tries)
	}
	if _, err := os.Stat(filepath.Join(home, ".ssh")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("rejected connections wrote to ~/.ssh: %v", err)
	}

	trust.HostKeys().Trust(server.Challenge())
	conn, err := connect(trust)
	if err != nil {
		t.Fatalf("connect after session approval: %v", err)
	}
	data, err := conn.ReadFile(filepath.Join(remoteDir, "file.txt"))
	if closeErr := conn.Close(); err != nil || closeErr != nil || string(data) != "content" {
		t.Fatalf("read = %q, %v, close = %v", data, err, closeErr)
	}
	// Session approval is not written anywhere, and another process has none.
	if _, err := os.Stat(filepath.Join(home, ".ssh")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("session approval wrote to ~/.ssh: %v", err)
	}
	if conn, err := connect(tlstrust.NewManager()); conn != nil || err == nil {
		t.Fatalf("a manager without the approval connected: %v", err)
	}

	if err := ssh.SaveHostKey(server.Challenge()); err != nil {
		t.Fatal(err)
	}
	conn, err = connect(nil)
	if err != nil {
		t.Fatalf("connect after saving the key: %v", err)
	}
	if err := conn.Close(); err != nil {
		t.Fatal(err)
	}
}
