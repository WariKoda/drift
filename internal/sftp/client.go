// Package sftp provides an SFTP client that wraps SSH connection + SFTP session.
package sftp

import (
	"context"
	"fmt"
	"io"
	"net"
	"os"
	"path"
	"strings"
	"time"

	pkgsftp "github.com/pkg/sftp"
	gossh "golang.org/x/crypto/ssh"

	"github.com/nibra180/drift-tui/internal/config"
	"github.com/nibra180/drift-tui/internal/ssh"
)

// Client holds an SSH connection and an SFTP session on top of it.
type Client struct {
	sshConn *gossh.Client
	sftp    *pkgsftp.Client
	Host    config.Host
}

// Connect dials SSH and opens an SFTP subsystem session.
func Connect(ctx context.Context, host config.Host) (*Client, error) {
	methods, err := ssh.AuthMethods(host.Auth)
	if err != nil {
		return nil, fmt.Errorf("auth setup: %w", err)
	}

	port := host.Port
	if port == 0 {
		port = 22
	}

	cfg := &gossh.ClientConfig{
		User:            host.User,
		Auth:            methods,
		HostKeyCallback: gossh.InsecureIgnoreHostKey(), // TODO Phase 5: known_hosts
		Timeout:         15 * time.Second,
	}

	addr := fmt.Sprintf("%s:%d", host.Hostname, port)
	// Prefer IPv4: "localhost" often resolves to ::1 first on dual-stack systems,
	// but many containers (e.g. dockware) only bind on 0.0.0.0, not :::.
	dialer := &net.Dialer{Timeout: cfg.Timeout}
	tcpConn, err := dialer.DialContext(ctx, "tcp4", addr)
	if err != nil {
		tcpConn, err = dialer.DialContext(ctx, "tcp", addr)
		if err != nil {
			return nil, fmt.Errorf("connect to %s: %w", addr, err)
		}
	}
	sshC, chans, reqs, err := gossh.NewClientConn(tcpConn, addr, cfg)
	if err != nil {
		tcpConn.Close()
		return nil, fmt.Errorf("connect to %s: %w", addr, err)
	}
	sshConn := gossh.NewClient(sshC, chans, reqs)

	sftpSession, err := pkgsftp.NewClient(sshConn)
	if err != nil {
		sshConn.Close()
		return nil, fmt.Errorf("open SFTP session: %w", err)
	}

	return &Client{sshConn: sshConn, sftp: sftpSession, Host: host}, nil
}

// Close closes both the SFTP session and SSH connection.
func (c *Client) Close() error {
	if c == nil {
		return nil
	}
	_ = c.sftp.Close()
	return c.sshConn.Close()
}

// Stat returns file info for a remote path.
func (c *Client) Stat(remotePath string) (os.FileInfo, error) {
	return c.sftp.Stat(remotePath)
}

// ReadFile reads the full content of a remote file.
func (c *Client) ReadFile(remotePath string) ([]byte, error) {
	f, err := c.sftp.Open(remotePath)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	return io.ReadAll(f)
}

// WriteFile writes data to a remote path, creating parent directories as needed.
func (c *Client) WriteFile(remotePath string, data []byte) error {
	if err := c.ensureDir(path.Dir(remotePath)); err != nil {
		return err
	}
	f, err := c.sftp.Create(remotePath)
	if err != nil {
		return fmt.Errorf("create %s: %w", remotePath, err)
	}
	defer f.Close()
	_, err = f.Write(data)
	return err
}

// UploadFile copies a local file to a remote path.
func (c *Client) UploadFile(localPath, remotePath string) error {
	src, err := os.Open(localPath)
	if err != nil {
		return fmt.Errorf("open local %s: %w", localPath, err)
	}
	defer src.Close()

	if err := c.ensureDir(path.Dir(remotePath)); err != nil {
		return err
	}
	dst, err := c.sftp.Create(remotePath)
	if err != nil {
		return fmt.Errorf("create remote %s: %w", remotePath, err)
	}
	defer dst.Close()

	_, err = io.Copy(dst, src)
	return err
}

// ensureDir creates remotePath and all missing parent directories.
// It is more resilient than sftp.MkdirAll: it walks each path component
// individually and uses Stat to skip components that already exist, working
// around SFTP servers that return "not a directory" instead of "not found"
// for missing path segments.
func (c *Client) ensureDir(remotePath string) error {
	// fast path
	if err := c.sftp.MkdirAll(remotePath); err == nil {
		return nil
	}
	// slow path: component-by-component
	parts := strings.Split(remotePath, "/")
	current := ""
	for _, part := range parts {
		if part == "" {
			if current == "" {
				current = "/"
			}
			continue
		}
		if current == "/" {
			current = "/" + part
		} else {
			current = current + "/" + part
		}
		info, err := c.sftp.Stat(current)
		if err == nil {
			if !info.IsDir() {
				return fmt.Errorf("ensureDir: %s exists but is not a directory", current)
			}
			continue // already a directory
		}
		if mkErr := c.sftp.Mkdir(current); mkErr != nil {
			// re-stat: might have been created concurrently
			if info2, statErr := c.sftp.Stat(current); statErr == nil && info2.IsDir() {
				continue
			}
			return fmt.Errorf("mkdir %s: %w", current, mkErr)
		}
	}
	return nil
}

// DeleteFile removes a file on the remote host.
func (c *Client) DeleteFile(remotePath string) error {
	return c.sftp.Remove(remotePath)
}

// WalkFiles calls fn for every regular file under remoteRoot, recursively.
// Unreadable entries are skipped silently.
func (c *Client) WalkFiles(remoteRoot string, fn func(path string) error) error {
	walker := c.sftp.Walk(remoteRoot)
	for walker.Step() {
		if walker.Err() != nil {
			continue
		}
		if !walker.Stat().IsDir() {
			if err := fn(walker.Path()); err != nil {
				return err
			}
		}
	}
	return nil
}

// DownloadFile copies a remote file to a local path.
func (c *Client) DownloadFile(remotePath, localPath string) error {
	src, err := c.sftp.Open(remotePath)
	if err != nil {
		return fmt.Errorf("open remote %s: %w", remotePath, err)
	}
	defer src.Close()

	if err := os.MkdirAll(path.Dir(localPath), 0o755); err != nil {
		return fmt.Errorf("mkdir %s: %w", path.Dir(localPath), err)
	}
	dst, err := os.Create(localPath)
	if err != nil {
		return fmt.Errorf("create local %s: %w", localPath, err)
	}
	defer dst.Close()

	_, err = io.Copy(dst, src)
	return err
}
