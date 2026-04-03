package ssh

import (
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"

	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

// HostKeyCallback returns a callback that verifies remote host keys against
// ~/.ssh/known_hosts (TOFU — Trust On First Use):
//
//   - Known host, key matches     → allowed
//   - Unknown host                → key is added to known_hosts, allowed
//   - Known host, key changed     → rejected with a clear error message
func HostKeyCallback() (gossh.HostKeyCallback, error) {
	path, err := knownHostsPath()
	if err != nil {
		return nil, err
	}

	// Create the file if it doesn't exist yet.
	if _, statErr := os.Stat(path); os.IsNotExist(statErr) {
		if mkErr := os.MkdirAll(filepath.Dir(path), 0o700); mkErr != nil {
			return nil, fmt.Errorf("create ~/.ssh: %w", mkErr)
		}
		f, createErr := os.OpenFile(path, os.O_CREATE, 0o600)
		if createErr != nil {
			return nil, fmt.Errorf("create known_hosts: %w", createErr)
		}
		f.Close()
	}

	checker, err := knownhosts.New(path)
	if err != nil {
		return nil, fmt.Errorf("parse known_hosts: %w", err)
	}

	return func(hostname string, remote net.Addr, key gossh.PublicKey) error {
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
	}, nil
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
