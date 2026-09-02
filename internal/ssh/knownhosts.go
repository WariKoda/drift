package ssh

import (
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"

	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

// HostKeyCallback returns a callback that verifies remote host keys against
// ~/.ssh/known_hosts (TOFU — Trust On First Use):
//
//   - Known host, key matches     → allowed
//   - Unknown host                → key is added to known_hosts, allowed
//   - Known host, key changed     → rejected with a clear error message
func HostKeyCallback(hostname string, port int) (gossh.HostKeyCallback, []string, error) {
	path, err := knownHostsPath()
	if err != nil {
		return nil, nil, err
	}

	// Create the file if it doesn't exist yet (atomic, no TOCTOU race).
	if mkErr := os.MkdirAll(filepath.Dir(path), 0o700); mkErr != nil {
		return nil, nil, fmt.Errorf("create ~/.ssh: %w", mkErr)
	}
	if f, createErr := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600); createErr == nil {
		f.Close()
	} else if !os.IsExist(createErr) {
		return nil, nil, fmt.Errorf("create known_hosts: %w", createErr)
	}

	checker, err := knownhosts.New(path)
	if err != nil {
		return nil, nil, fmt.Errorf("parse known_hosts: %w", err)
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

		// Host is not yet known — add it (TOFU).
		return addKnownHost(path, hostname, key)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, fmt.Errorf("read known_hosts: %w", err)
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
		if key.Type() == gossh.KeyAlgoRSA {
			addAlgorithm(gossh.KeyAlgoRSASHA512)
			addAlgorithm(gossh.KeyAlgoRSASHA256)
		}
		addAlgorithm(key.Type())
	}

	if len(algorithms) > 0 {
		for _, algorithm := range gossh.SupportedAlgorithms().HostKeys {
			addAlgorithm(algorithm)
		}
	}

	return callback, algorithms, nil
}

func addKnownHost(path, hostname string, key gossh.PublicKey) error {
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return fmt.Errorf("write known_hosts: %w", err)
	}
	defer f.Close()

	line := knownhosts.Line([]string{knownhosts.Normalize(hostname)}, key)
	_, err = fmt.Fprintln(f, line)
	return err
}

func knownHostsPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("find home dir: %w", err)
	}
	return filepath.Join(home, ".ssh", "known_hosts"), nil
}
