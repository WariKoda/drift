// Package ssh provides SSH authentication helpers.
package ssh

import (
	"context"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"time"

	gossh "golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/agent"

	"github.com/WariKoda/drift/internal/config"
)

// AuthMethods builds the list of SSH auth methods for a host config.
// The returned io.Closer must be closed when the SSH connection is done
// to release any resources (e.g. the SSH agent socket). It may be nil.
//
// ctx bounds the work against the SSH agent, which happens during the
// handshake that follows.
func AuthMethods(ctx context.Context, auth config.Auth) ([]gossh.AuthMethod, io.Closer, error) {
	switch auth.Type {
	case "agent", "":
		return agentAuth(ctx)
	case "keyfile":
		methods, err := keyfileAuth(ctx, auth)
		return methods, nil, err
	case "password":
		pass := os.ExpandEnv(auth.Password)
		return []gossh.AuthMethod{gossh.Password(pass)}, nil, nil
	default:
		return nil, nil, fmt.Errorf("unknown auth type %q (use keyfile, password, or agent)", auth.Type)
	}
}

func keyfileAuth(ctx context.Context, auth config.Auth) ([]gossh.AuthMethod, error) {
	path := expandHome(os.ExpandEnv(auth.KeyFile))
	if path == "" {
		// fall back to agent if no key file configured
		methods, _, err := agentAuth(ctx)
		return methods, err
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

// agentTimeout bounds agent requests when the caller passed a context without a
// deadline. The agent runs on the same machine, so anything slower than this is
// a stuck agent, not a slow one.
const agentTimeout = 15 * time.Second

func agentAuth(ctx context.Context) ([]gossh.AuthMethod, io.Closer, error) {
	sock := os.Getenv("SSH_AUTH_SOCK")
	if sock == "" {
		return nil, nil, fmt.Errorf("SSH_AUTH_SOCK not set and no keyfile configured")
	}
	var dialer net.Dialer
	conn, err := dialer.DialContext(ctx, "unix", sock)
	if err != nil {
		return nil, nil, fmt.Errorf("connect to SSH agent: %w", err)
	}
	// Listing the keys and signing with them happen on this socket during the
	// handshake. Without a deadline an agent that stops answering, a smartcard
	// waiting for a PIN for instance, holds the connect attempt open for good:
	// the connect timeout covers the network dial, not these requests.
	deadline, ok := ctx.Deadline()
	if !ok {
		deadline = time.Now().Add(agentTimeout)
	}
	if err := conn.SetDeadline(deadline); err != nil {
		conn.Close()
		return nil, nil, fmt.Errorf("set SSH agent deadline: %w", err)
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
