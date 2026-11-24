package ssh

import (
	"crypto/ecdsa"
	"crypto/ed25519"
	"crypto/elliptic"
	"crypto/rand"
	"errors"
	"net"
	"os"
	"path/filepath"
	"strings"
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

	callback, algorithms, err := HostKeyCallback(hostname, 22, nil)
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

	_, algorithms, err := HostKeyCallback("unknown.example.com", 22, nil)
	if err != nil {
		t.Fatal(err)
	}
	if algorithms != nil {
		t.Fatalf("algorithms = %v, want nil for SSH defaults", algorithms)
	}
}

// An unknown host used to be added to known_hosts without a question, so the
// first connection sent the password to whoever answered.
func TestUnknownHostIsRejectedAndNothingIsWritten(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	key := testHostKey(t)

	callback, _, err := HostKeyCallback("new.example.com", 2222, nil)
	if err != nil {
		t.Fatal(err)
	}
	err = callback("new.example.com:2222", testRemote, key)
	var unknown *UnknownHostKeyError
	if !errors.As(err, &unknown) {
		t.Fatalf("error = %v, want an unknown host key error", err)
	}
	if unknown.Challenge.Address != "[new.example.com]:2222" || !unknown.Challenge.Equal(HostKeyChallenge{Address: "[new.example.com]:2222", Key: key}) {
		t.Fatalf("challenge = %+v", unknown.Challenge)
	}
	if !strings.HasPrefix(unknown.Challenge.Fingerprint(), "SHA256:") || unknown.Challenge.KeyType() != gossh.KeyAlgoED25519 {
		t.Fatalf("fingerprint %q, type %q", unknown.Challenge.Fingerprint(), unknown.Challenge.KeyType())
	}
	if _, statErr := os.Stat(filepath.Join(home, ".ssh")); !errors.Is(statErr, os.ErrNotExist) {
		t.Fatalf("connecting created ~/.ssh: %v", statErr)
	}
}

func TestSessionTrustAcceptsOnlyTheApprovedKey(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	approved, other := testHostKey(t), testHostKey(t)
	session := NewSessionHostKeys()
	session.Trust(HostKeyChallenge{Address: "session.example.com", Key: approved})

	callback, algorithms, err := HostKeyCallback("session.example.com", 22, session)
	if err != nil {
		t.Fatal(err)
	}
	if len(algorithms) == 0 || algorithms[0] != gossh.KeyAlgoED25519 {
		t.Fatalf("algorithms = %v, want the approved key type first", algorithms)
	}
	if err := callback("session.example.com:22", testRemote, approved); err != nil {
		t.Fatalf("approved key rejected: %v", err)
	}
	err = callback("session.example.com:22", testRemote, other)
	var unknown *UnknownHostKeyError
	if err == nil || errors.As(err, &unknown) || !strings.Contains(err.Error(), "has changed") {
		t.Fatalf("different key: error = %v, want a changed-key rejection without a new prompt", err)
	}

	// Another port is another endpoint.
	elsewhere, _, err := HostKeyCallback("session.example.com", 2222, session)
	if err != nil {
		t.Fatal(err)
	}
	if err := elsewhere("session.example.com:2222", testRemote, approved); !errors.As(err, &unknown) {
		t.Fatalf("other port: error = %v, want an unknown host key error", err)
	}
}

func TestSaveHostKeyWritesOneEntryThatOpenSSHCanRead(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	key := testHostKey(t)
	challenge := HostKeyChallenge{Address: "[saved.example.com]:2222", Key: key}

	for range 2 {
		if err := SaveHostKey(challenge); err != nil {
			t.Fatal(err)
		}
	}
	path := filepath.Join(home, ".ssh", "known_hosts")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if want := knownhosts.Line([]string{"[saved.example.com]:2222"}, key) + "\n"; string(data) != want {
		t.Fatalf("known_hosts = %q, want %q", data, want)
	}
	for name, want := range map[string]os.FileMode{path: 0o600, filepath.Dir(path): 0o700} {
		info, err := os.Stat(name)
		if err != nil || info.Mode().Perm() != want {
			t.Fatalf("%s mode = %v, %v; want %v", name, info.Mode().Perm(), err, want)
		}
	}

	callback, algorithms, err := HostKeyCallback("saved.example.com", 2222, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(algorithms) == 0 || algorithms[0] != gossh.KeyAlgoED25519 {
		t.Fatalf("algorithms = %v, want the saved key type first", algorithms)
	}
	if err := callback("saved.example.com:2222", testRemote, key); err != nil {
		t.Fatalf("saved key rejected: %v", err)
	}
	if err := callback("saved.example.com:2222", testRemote, testHostKey(t)); err == nil || !strings.Contains(err.Error(), "has changed") {
		t.Fatalf("changed key: error = %v", err)
	}
	if err := SaveHostKey(HostKeyChallenge{Address: "[saved.example.com]:2222", Key: testHostKey(t)}); err == nil ||
		!strings.Contains(err.Error(), "different key") {
		t.Fatalf("saving a second key: error = %v", err)
	}
}

func TestSaveHostKeyKeepsExistingEntriesIntact(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	sshDir := filepath.Join(home, ".ssh")
	if err := os.Mkdir(sshDir, 0o700); err != nil {
		t.Fatal(err)
	}
	existing, added := testHostKey(t), testHostKey(t)
	// No newline at the end, as an editor may leave it.
	first := knownhosts.Line([]string{"first.example.com"}, existing)
	path := filepath.Join(sshDir, "known_hosts")
	if err := os.WriteFile(path, []byte(first), 0o600); err != nil {
		t.Fatal(err)
	}

	if err := SaveHostKey(HostKeyChallenge{Address: "second.example.com", Key: added}); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if want := first + "\n" + knownhosts.Line([]string{"second.example.com"}, added) + "\n"; string(data) != want {
		t.Fatalf("known_hosts = %q, want %q", data, want)
	}
	for host, key := range map[string]gossh.PublicKey{"first.example.com": existing, "second.example.com": added} {
		callback, _, err := HostKeyCallback(host, 22, nil)
		if err != nil {
			t.Fatal(err)
		}
		if err := callback(host+":22", testRemote, key); err != nil {
			t.Fatalf("%s rejected after the save: %v", host, err)
		}
	}
}

// A server with several keys must be asked for the type that was approved.
func TestSessionKeyTypeIsPreferredOverDefaults(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	private, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(private)
	if err != nil {
		t.Fatal(err)
	}
	session := NewSessionHostKeys()
	session.Trust(HostKeyChallenge{Address: "::1", Key: signer.PublicKey()})
	_, algorithms, err := HostKeyCallback("[::1]", 22, session)
	if err != nil {
		t.Fatal(err)
	}
	if len(algorithms) == 0 || algorithms[0] != gossh.KeyAlgoECDSA256 {
		t.Fatalf("algorithms = %v, want %q first", algorithms, gossh.KeyAlgoECDSA256)
	}
}

var testRemote = &net.TCPAddr{IP: net.IPv4(192, 0, 2, 1), Port: 22}

func testHostKey(t *testing.T) gossh.PublicKey {
	t.Helper()
	_, privateKey, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(privateKey)
	if err != nil {
		t.Fatal(err)
	}
	return signer.PublicKey()
}
