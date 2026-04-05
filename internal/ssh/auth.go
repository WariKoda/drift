// Package ssh provides SSH authentication helpers.
package ssh

import (
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"

	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/agent"

	"github.com/nibra180/drift-tui/internal/config"
)

// AuthMethods builds the list of SSH auth methods for a host config.
// The returned io.Closer must be closed when the SSH connection is done
// to release any resources (e.g. the SSH agent socket). It may be nil.
func AuthMethods(auth config.Auth) ([]gossh.AuthMethod, io.Closer, error) {
	switch auth.Type {
	case "agent", "":
		return agentAuth()
	case "keyfile":
		methods, err := keyfileAuth(auth)
		return methods, nil, err
	case "password":
		pass := os.ExpandEnv(auth.Password)
		return []gossh.AuthMethod{gossh.Password(pass)}, nil, nil
	default:
		return nil, nil, fmt.Errorf("unknown auth type %q (use keyfile, password, or agent)", auth.Type)
	}
}

func keyfileAuth(auth config.Auth) ([]gossh.AuthMethod, error) {
	path := expandHome(os.ExpandEnv(auth.KeyFile))
	if path == "" {
		// fall back to agent if no key file configured
		return agentAuth()
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read key file %s: %w", path, err)
	}

	passphrase := os.ExpandEnv(auth.Passphrase)
	var signer gossh.Signer
	if passphrase != "" {
		signer, err = gossh.ParsePrivateKeyWithPassphrase(data, []byte(passphrase))
	} else {
		signer, err = gossh.ParsePrivateKey(data)
	}
	if err != nil {
		return nil, fmt.Errorf("parse private key %s: %w", path, err)
	}

	return []gossh.AuthMethod{gossh.PublicKeys(signer)}, nil
}

func agentAuth() ([]gossh.AuthMethod, io.Closer, error) {
	sock := os.Getenv("SSH_AUTH_SOCK")
	if sock == "" {
		return nil, nil, fmt.Errorf("SSH_AUTH_SOCK not set and no keyfile configured")
	}
	conn, err := net.Dial("unix", sock)
	if err != nil {
		return nil, nil, fmt.Errorf("connect to SSH agent: %w", err)
	}
	return []gossh.AuthMethod{
		gossh.PublicKeysCallback(agent.NewClient(conn).Signers),
	}, conn, nil
}

func expandHome(path string) string {
	if path == "" {
		return ""
	}
	if path[:1] == "~" {
		home, _ := os.UserHomeDir()
		return filepath.Join(home, path[1:])
	}
	return path
}
