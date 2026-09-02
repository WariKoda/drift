// Package sftp provides an SFTP client that wraps SSH connection + SFTP session.
package sftp

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path"
	"sort"
	"strings"
	"time"

	pkgsftp "github.com/pkg/sftp"
	gossh "golang.org/x/crypto/ssh"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ssh"
)

// Client holds an SSH connection and an SFTP session on top of it.
type Client struct {
	sshConn   *gossh.Client
	sftp      *pkgsftp.Client
	authClose io.Closer // optional closer for auth resources (e.g. SSH agent socket)
	Host      config.Host
}

// Connect dials SSH and opens an SFTP subsystem session.
func Connect(ctx context.Context, host config.Host) (*Client, error) {
	methods, authCloser, err := ssh.AuthMethods(host.Auth)
	if err != nil {
		return nil, fmt.Errorf("auth setup: %w", err)
	}

	port := host.Port
	if port == 0 {
		port = 22
	}

	hkc, hostKeyAlgorithms, err := ssh.HostKeyCallback(host.Hostname, port)
	if err != nil {
		return nil, fmt.Errorf("known_hosts: %w", err)
	}

	cfg := &gossh.ClientConfig{
		User:              host.User,
		Auth:              methods,
		HostKeyCallback:   hkc,
		HostKeyAlgorithms: hostKeyAlgorithms,
		Timeout:           15 * time.Second,
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

	return &Client{sshConn: sshConn, sftp: sftpSession, authClose: authCloser, Host: host}, nil
}

// Close closes both the SFTP session and SSH connection.
func (c *Client) Close() error {
	if c == nil {
		return nil
	}
	_ = c.sftp.Close()
	err := c.sshConn.Close()
	if c.authClose != nil {
		_ = c.authClose.Close()
	}
	return err
}

// Stat returns file info for a remote path.
func (c *Client) Stat(remotePath string) (os.FileInfo, error) {
	return c.sftp.Stat(remotePath)
}

// ReadDir reads one remote directory level.
// Directories are returned before files; both groups sorted alphabetically.
func (c *Client) ReadDir(remotePath string) ([]*fs.FileEntry, error) {
	infos, err := c.sftp.ReadDir(remotePath)
	if err != nil {
		return nil, err
	}

	var dirs, files []*fs.FileEntry
	for _, info := range infos {
		kind := fs.EntryFile
		switch {
		case info.IsDir():
			kind = fs.EntryDir
		case info.Mode()&os.ModeSymlink != 0:
			kind = fs.EntrySymlink
		}
		entry := &fs.FileEntry{
			Name:    info.Name(),
			Path:    path.Join(remotePath, info.Name()),
			Kind:    kind,
			Size:    info.Size(),
			ModTime: info.ModTime(),
			Mode:    info.Mode(),
		}
		if kind == fs.EntryDir {
			dirs = append(dirs, entry)
		} else {
			files = append(files, entry)
		}
	}

	sort.Slice(dirs, func(i, j int) bool { return dirs[i].Name < dirs[j].Name })
	sort.Slice(files, func(i, j int) bool { return files[i].Name < files[j].Name })
	return append(dirs, files...), nil
}

// Open opens a remote file for streaming reads.
func (c *Client) Open(remotePath string) (io.ReadCloser, error) {
	f, err := c.sftp.Open(remotePath)
	if err != nil {
		return nil, err
	}
	return f, nil
}

// ReadFile reads the full content of a remote file.
func (c *Client) ReadFile(remotePath string) ([]byte, error) {
	f, err := c.Open(remotePath)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	return io.ReadAll(f)
}

// Upload atomically writes everything src yields to a remote path, creating
// parent directories as needed. The existing target is replaced only after the
// staged file has been written and closed successfully, and keeps its
// permission bits. A target that is not a regular file is refused.
func (c *Client) Upload(remotePath string, src io.Reader) error {
	if err := c.ensureDir(path.Dir(remotePath)); err != nil {
		return err
	}

	var mode os.FileMode
	preserveMode := false
	if info, statErr := c.sftp.Lstat(remotePath); statErr == nil {
		if !info.Mode().IsRegular() {
			return fmt.Errorf("remote target %s is not a regular file", remotePath)
		}
		mode = info.Mode().Perm()
		preserveMode = true
	} else if !os.IsNotExist(statErr) {
		return fmt.Errorf("stat remote %s: %w", remotePath, statErr)
	}

	stageBase, err := stagingName(path.Base(remotePath))
	if err != nil {
		return err
	}
	stagePath := path.Join(path.Dir(remotePath), stageBase)
	dst, err := c.sftp.OpenFile(stagePath, os.O_WRONLY|os.O_CREATE|os.O_EXCL)
	if err != nil {
		return fmt.Errorf("create staged remote file for %s: %w", remotePath, err)
	}
	committed := false
	defer func() {
		if !committed {
			_ = c.sftp.Remove(stagePath)
		}
	}()

	_, copyErr := io.Copy(dst, src)
	var chmodErr error
	if copyErr == nil && preserveMode {
		chmodErr = dst.Chmod(mode)
	}
	closeErr := dst.Close()

	var transferErr error
	if copyErr != nil {
		transferErr = errors.Join(transferErr, fmt.Errorf("upload staged remote file for %s: %w", remotePath, copyErr))
	}
	if chmodErr != nil {
		transferErr = errors.Join(transferErr, fmt.Errorf("preserve remote mode for %s: %w", remotePath, chmodErr))
	}
	if closeErr != nil {
		transferErr = errors.Join(transferErr, fmt.Errorf("close staged remote file for %s: %w", remotePath, closeErr))
	}
	if transferErr != nil {
		return transferErr
	}

	if posixErr := c.sftp.PosixRename(stagePath, remotePath); posixErr != nil {
		if renameErr := c.sftp.Rename(stagePath, remotePath); renameErr != nil {
			return fmt.Errorf(
				"replace remote %s (posix rename: %v): %w",
				remotePath, posixErr, renameErr,
			)
		}
	}
	committed = true
	return nil
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
// A listing error on any directory is propagated rather than skipped: swallowing
// it would silently drop that subtree from the walk, so its files would never be
// compared or synced.
func (c *Client) WalkFiles(remoteRoot string, fn func(path string) error) error {
	walker := c.sftp.Walk(remoteRoot)
	for walker.Step() {
		if err := walker.Err(); err != nil {
			return fmt.Errorf("walk %s: %w", walker.Path(), err)
		}
		if walker.Stat().IsDir() {
			if walker.Path() != remoteRoot && fs.ShouldSkipDir(path.Base(walker.Path())) {
				walker.SkipDir()
			}
			continue
		}
		if err := fn(walker.Path()); err != nil {
			return err
		}
	}
	return nil
}

// stagingName returns an unpredictable hidden sibling name. Staging files must
// live beside their target so the final rename stays on the same filesystem.
func stagingName(base string) (string, error) {
	var token [16]byte
	if _, err := rand.Read(token[:]); err != nil {
		return "", fmt.Errorf("generate staging name: %w", err)
	}
	return "." + base + ".drift-tmp-" + hex.EncodeToString(token[:]), nil
}
