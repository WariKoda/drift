package ssh

import (
	"crypto/ed25519"
	"crypto/rand"
	"net"
	"os"
	"path/filepath"
	"testing"

	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

func TestHostKeyCallbackPrioritizesKnownAlgorithm(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	sshDir := filepath.Join(home, ".ssh")
	if err := os.Mkdir(sshDir, 0o700); err != nil {
		t.Fatal(err)
	}

	_, privateKey, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(privateKey)
	if err != nil {
		t.Fatal(err)
	}
	hostname := "example.com"
	line := knownhosts.Line([]string{hostname}, signer.PublicKey())
	if err := os.WriteFile(filepath.Join(sshDir, "known_hosts"), []byte(line+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	callback, algorithms, err := HostKeyCallback(hostname, 22)
	if err != nil {
		t.Fatal(err)
	}
	if len(algorithms) == 0 || algorithms[0] != gossh.KeyAlgoED25519 {
		t.Fatalf("first host key algorithm = %v, want %q", algorithms, gossh.KeyAlgoED25519)
	}
	if err := callback(net.JoinHostPort(hostname, "22"), &net.TCPAddr{IP: net.IPv4(192, 0, 2, 1), Port: 22}, signer.PublicKey()); err != nil {
		t.Fatalf("callback rejected known key: %v", err)
	}
}

func TestHostKeyCallbackUsesDefaultsForUnknownHost(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	_, algorithms, err := HostKeyCallback("unknown.example.com", 22)
	if err != nil {
		t.Fatal(err)
	}
	if algorithms != nil {
		t.Fatalf("algorithms = %v, want nil for SSH defaults", algorithms)
	}
}
