package sftp

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/ssh"
)

// The first connection to a host used to store its key and go on to send the
// password. Whoever answered that connection received it.
func TestUnknownHostKeyStopsBeforeAuthentication(t *testing.T) {
	server := newLocalSSHServer(t, false)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	client, err := connect(ctx, server.host, nil, 0, time.Second)
	if client != nil {
		_ = client.Close()
	}
	var unknown *ssh.UnknownHostKeyError
	if !errors.As(err, &unknown) {
		t.Fatalf("error = %v, want an unknown host key error", err)
	}
	if !unknown.Challenge.Equal(server.challenge) {
		t.Fatalf("challenge = %s %s, want the server's key for %s",
			unknown.Challenge.Address, unknown.Challenge.Fingerprint(), server.challenge.Address)
	}
	waitSignal(t, server.done, "server side of the rejected handshake")
	if tries := server.authTries.Load(); tries != 0 {
		t.Fatalf("server received %d password attempt(s) before the key was approved", tries)
	}
	if _, statErr := os.Stat(filepath.Join(os.Getenv("HOME"), ".ssh")); !errors.Is(statErr, os.ErrNotExist) {
		t.Fatalf("the rejected connection wrote to ~/.ssh: %v", statErr)
	}
}

func TestSavedHostKeyAllowsTheNextConnection(t *testing.T) {
	server := newLocalSSHServer(t, false)
	if err := ssh.SaveHostKey(server.challenge); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	client, err := connect(ctx, server.host, nil, 0, time.Second)
	if err != nil {
		t.Fatalf("connect after saving the key: %v", err)
	}
	if err := client.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestChangedHostKeyIsRejectedWithoutAPrompt(t *testing.T) {
	server := newLocalSSHServer(t, false)
	other := newLocalSSHServer(t, false)
	// known_hosts lists another key for this address.
	stale := ssh.HostKeyChallenge{Address: server.challenge.Address, Key: other.challenge.Key}
	if err := ssh.SaveHostKey(stale); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	client, err := connect(ctx, server.host, nil, 0, time.Second)
	if client != nil {
		_ = client.Close()
	}
	var unknown *ssh.UnknownHostKeyError
	if err == nil || errors.As(err, &unknown) {
		t.Fatalf("error = %v, want a rejection that offers no prompt", err)
	}
	if tries := server.authTries.Load(); tries != 0 {
		t.Fatalf("server received %d password attempt(s)", tries)
	}
}
