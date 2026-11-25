package ssh

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"

	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

// HostKeyChallenge describes a host key that known_hosts does not list, for a
// trust prompt. Address has the form known_hosts uses: the bare host name for
// port 22, "[host]:port" otherwise.
type HostKeyChallenge struct {
	Address string
	Key     gossh.PublicKey
}

// KeyType names the key algorithm, such as "ssh-ed25519".
func (c HostKeyChallenge) KeyType() string { return c.Key.Type() }

// Fingerprint returns the SHA-256 fingerprint as OpenSSH prints it, so it can
// be compared with the output of ssh-keygen -lf on the server.
func (c HostKeyChallenge) Fingerprint() string { return gossh.FingerprintSHA256(c.Key) }

// Equal reports whether both challenges name the same key for the same address.
func (c HostKeyChallenge) Equal(other HostKeyChallenge) bool {
	return c.Address == other.Address && c.Key != nil && other.Key != nil &&
		bytes.Equal(c.Key.Marshal(), other.Key.Marshal())
}

// UnknownHostKeyError reports a host that known_hosts does not list and the
// user has not approved. The connection stops before authentication, so no
// password reaches a server whose key nobody has checked.
type UnknownHostKeyError struct {
	Challenge HostKeyChallenge
}

func (e *UnknownHostKeyError) Error() string {
	return fmt.Sprintf("unknown SSH host key for %s: %s %s",
		e.Challenge.Address, e.Challenge.KeyType(), e.Challenge.Fingerprint())
}

// SessionHostKeys holds host keys the user approved for this process only.
// A nil store trusts nothing.
type SessionHostKeys struct {
	mu   sync.RWMutex
	keys map[string]gossh.PublicKey
}

// NewSessionHostKeys creates an empty store.
func NewSessionHostKeys() *SessionHostKeys {
	return &SessionHostKeys{keys: make(map[string]gossh.PublicKey)}
}

// Trust accepts the challenge's key for its address until the process exits.
func (s *SessionHostKeys) Trust(challenge HostKeyChallenge) {
	s.mu.Lock()
	s.keys[challenge.Address] = challenge.Key
	s.mu.Unlock()
}

func (s *SessionHostKeys) key(address string) (gossh.PublicKey, bool) {
	if s == nil {
		return nil, false
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	key, ok := s.keys[address]
	return key, ok
}

// HostKeyCallback returns a callback that verifies remote host keys against
// ~/.ssh/known_hosts and the keys approved for this session:
//
//   - Known host, key matches     → allowed
//   - Unknown host                → rejected with *UnknownHostKeyError
//   - Known host, key changed     → rejected with a clear error message
//
// Nothing is written here. The caller shows the challenge to the user and
// stores an approved key with SaveHostKey or SessionHostKeys.Trust.
//
// The returned algorithms put the key types already trusted for the host first,
// so the server offers the key that can be checked.
func HostKeyCallback(hostname string, port int, session *SessionHostKeys) (gossh.HostKeyCallback, []string, error) {
	// The dialled address carries an IPv6 literal without brackets.
	hostname = strings.Trim(hostname, "[]")
	path, err := knownHostsPath()
	if err != nil {
		return nil, nil, err
	}
	data, err := os.ReadFile(path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return nil, nil, fmt.Errorf("read known_hosts: %w", err)
	}
	checker, err := knownHostsChecker(path)
	if err != nil {
		return nil, nil, err
	}

	callback := func(hostname string, remote net.Addr, key gossh.PublicKey) error {
		err := checker(hostname, remote, key)
		if err == nil {
			return nil
		}

		var keyErr *knownhosts.KeyError
		if !errors.As(err, &keyErr) {
			return err
		}

		if len(keyErr.Want) > 0 {
			// Host is known but the key no longer matches.
			return fmt.Errorf(
				"WARNING: remote host identification has changed for %s\n"+
					"Possible MITM attack. Remove the old key from %s to connect.",
				hostname, path,
			)
		}

		address := knownhosts.Normalize(hostname)
		if trusted, ok := session.key(address); ok {
			if bytes.Equal(trusted.Marshal(), key.Marshal()) {
				return nil
			}
			return fmt.Errorf(
				"WARNING: remote host identification has changed for %s\n"+
					"Possible MITM attack. The key differs from the one trusted for this session.",
				hostname,
			)
		}
		return &UnknownHostKeyError{Challenge: HostKeyChallenge{Address: address, Key: key}}
	}

	address := net.JoinHostPort(hostname, strconv.Itoa(port))
	remote := &net.TCPAddr{IP: net.IPv4zero, Port: port}
	seen := make(map[string]bool)
	var algorithms []string
	addAlgorithm := func(algorithm string) {
		if !seen[algorithm] {
			seen[algorithm] = true
			algorithms = append(algorithms, algorithm)
		}
	}

	for len(data) > 0 {
		_, _, key, _, rest, parseErr := gossh.ParseKnownHosts(data)
		if errors.Is(parseErr, io.EOF) {
			break
		}
		if parseErr != nil {
			return nil, nil, fmt.Errorf("parse known_hosts: %w", parseErr)
		}
		data = rest
		if checker(address, remote, key) != nil {
			continue
		}
		addKeyAlgorithms(key, addAlgorithm)
	}

	// Without this the server may offer another of its keys, which would look
	// like a changed host.
	if trusted, ok := session.key(knownhosts.Normalize(address)); ok {
		addKeyAlgorithms(trusted, addAlgorithm)
	}

	if len(algorithms) > 0 {
		for _, algorithm := range gossh.SupportedAlgorithms().HostKeys {
			addAlgorithm(algorithm)
		}
	}

	return callback, algorithms, nil
}

func addKeyAlgorithms(key gossh.PublicKey, add func(string)) {
	if key.Type() == gossh.KeyAlgoRSA {
		add(gossh.KeyAlgoRSASHA512)
		add(gossh.KeyAlgoRSASHA256)
	}
	add(key.Type())
}

// knownHostsChecker parses path. A missing file is an empty list: connecting
// must not create ~/.ssh/known_hosts for a host the user may still reject.
func knownHostsChecker(path string) (gossh.HostKeyCallback, error) {
	files := []string{path}
	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) {
		files = nil
	}
	checker, err := knownhosts.New(files...)
	if err != nil {
		return nil, fmt.Errorf("parse known_hosts: %w", err)
	}
	return checker, nil
}

// SaveHostKey appends an approved key to ~/.ssh/known_hosts, which OpenSSH
// reads too. It writes nothing if the file already lists the key, and fails if
// the file lists a different key for the address.
func SaveHostKey(challenge HostKeyChallenge) error {
	path, err := knownHostsPath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("create ~/.ssh: %w", err)
	}
	checker, err := knownHostsChecker(path)
	if err != nil {
		return err
	}
	// The checker wants a dialled address; Normalize removed port 22.
	dialled := challenge.Address
	if _, _, splitErr := net.SplitHostPort(dialled); splitErr != nil {
		dialled = net.JoinHostPort(dialled, "22")
	}
	checkErr := checker(dialled, &net.TCPAddr{IP: net.IPv4zero}, challenge.Key)
	if checkErr == nil {
		return nil
	}
	var keyErr *knownhosts.KeyError
	if !errors.As(checkErr, &keyErr) {
		return checkErr
	}
	if len(keyErr.Want) > 0 {
		return fmt.Errorf("%s already lists a different key for %s", path, challenge.Address)
	}

	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return fmt.Errorf("write known_hosts: %w", err)
	}
	defer f.Close()
	line := knownhosts.Line([]string{challenge.Address}, challenge.Key) + "\n"
	// A file without a final newline would otherwise get the new entry glued
	// onto its last line, which breaks both.
	if info, statErr := f.Stat(); statErr == nil && info.Size() > 0 {
		last := make([]byte, 1)
		if _, readErr := f.ReadAt(last, info.Size()-1); readErr == nil && last[0] != '\n' {
			line = "\n" + line
		}
	}
	if _, err := f.WriteString(line); err != nil {
		return fmt.Errorf("write known_hosts: %w", err)
	}
	return f.Close()
}

func knownHostsPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("find home dir: %w", err)
	}
	return filepath.Join(home, ".ssh", "known_hosts"), nil
}
