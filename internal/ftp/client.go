// Package ftp provides an FTP client that implements remote.Client.
package ftp

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"path"
	"strings"
	"time"

	ftplib "github.com/jlaffaye/ftp"

	"github.com/nibra180/drift-tui/internal/config"
)

// Client wraps an FTP connection.
type Client struct {
	conn *ftplib.ServerConn
	Host config.Host
}

// Connect dials an FTP server and logs in.
func Connect(ctx context.Context, host config.Host) (*Client, error) {
	port := host.Port
	if port == 0 {
		port = 21
	}
	addr := fmt.Sprintf("%s:%d", host.Hostname, port)

	conn, err := ftplib.Dial(addr,
		ftplib.DialWithContext(ctx),
		ftplib.DialWithTimeout(15*time.Second),
	)
	if err != nil {
		return nil, fmt.Errorf("connect to %s: %w", addr, err)
	}

	pass := os.ExpandEnv(host.Auth.Password)
	if err := conn.Login(host.User, pass); err != nil {
		_ = conn.Quit()
		return nil, fmt.Errorf("login to %s: %w", addr, err)
	}

	return &Client{conn: conn, Host: host}, nil
}

// Close logs out and closes the connection.
func (c *Client) Close() error {
	if c == nil {
		return nil
	}
	return c.conn.Quit()
}

// Stat returns file info for a remote path.
func (c *Client) Stat(remotePath string) (os.FileInfo, error) {
	size, err := c.conn.FileSize(remotePath)
	if err != nil {
		// Check if it's a directory by attempting to list it.
		entries, listErr := c.conn.List(remotePath)
		if listErr != nil {
			return nil, err
		}
		_ = entries
		return &ftpFileInfo{name: path.Base(remotePath), isDir: true}, nil
	}
	t, _ := c.conn.GetTime(remotePath) // MDTM — may return zero on unsupported servers
	return &ftpFileInfo{
		name:    path.Base(remotePath),
		size:    size,
		modTime: t,
	}, nil
}

// ReadFile reads the full contents of a remote file.
func (c *Client) ReadFile(remotePath string) ([]byte, error) {
	r, err := c.conn.Retr(remotePath)
	if err != nil {
		return nil, fmt.Errorf("retr %s: %w", remotePath, err)
	}
	defer r.Close()
	return io.ReadAll(r)
}

// WriteFile writes data to a remote path, creating parent directories as needed.
func (c *Client) WriteFile(remotePath string, data []byte) error {
	if err := c.ensureDir(path.Dir(remotePath)); err != nil {
		return err
	}
	return c.conn.Stor(remotePath, bytes.NewReader(data))
}

// UploadFile copies a local file to a remote path.
func (c *Client) UploadFile(localPath, remotePath string) error {
	if err := c.ensureDir(path.Dir(remotePath)); err != nil {
		return err
	}
	f, err := os.Open(localPath)
	if err != nil {
		return fmt.Errorf("open local %s: %w", localPath, err)
	}
	defer f.Close()
	if err := c.conn.Stor(remotePath, f); err != nil {
		return fmt.Errorf("stor %s: %w", remotePath, err)
	}
	return nil
}

// DownloadFile copies a remote file to a local path.
func (c *Client) DownloadFile(remotePath, localPath string) error {
	r, err := c.conn.Retr(remotePath)
	if err != nil {
		return fmt.Errorf("retr %s: %w", remotePath, err)
	}
	defer r.Close()

	if err := os.MkdirAll(path.Dir(localPath), 0o755); err != nil {
		return fmt.Errorf("mkdir %s: %w", path.Dir(localPath), err)
	}
	f, err := os.Create(localPath)
	if err != nil {
		return fmt.Errorf("create local %s: %w", localPath, err)
	}
	defer f.Close()
	_, err = io.Copy(f, r)
	return err
}

// WalkFiles calls fn for every regular file under remoteRoot, recursively.
func (c *Client) WalkFiles(remoteRoot string, fn func(string) error) error {
	return c.walkDir(remoteRoot, fn)
}

func (c *Client) walkDir(dir string, fn func(string) error) error {
	entries, err := c.conn.List(dir)
	if err != nil {
		return nil // skip unreadable directories
	}
	for _, e := range entries {
		if e.Name == "." || e.Name == ".." {
			continue
		}
		p := strings.TrimSuffix(dir, "/") + "/" + e.Name
		switch e.Type {
		case ftplib.EntryTypeFolder:
			_ = c.walkDir(p, fn)
		case ftplib.EntryTypeFile:
			if err := fn(p); err != nil {
				return err
			}
		}
	}
	return nil
}

// DeleteFile removes a file on the remote host.
func (c *Client) DeleteFile(remotePath string) error {
	return c.conn.Delete(remotePath)
}

// ensureDir creates all path components that do not yet exist.
func (c *Client) ensureDir(dir string) error {
	dir = strings.TrimSuffix(dir, "/")
	if dir == "" || dir == "." {
		return nil
	}
	parts := strings.Split(strings.TrimPrefix(dir, "/"), "/")
	current := ""
	if strings.HasPrefix(dir, "/") {
		current = "/"
	}
	for _, part := range parts {
		if part == "" {
			continue
		}
		if current == "/" {
			current = "/" + part
		} else {
			current = current + "/" + part
		}
		_ = c.conn.MakeDir(current) // ignore "already exists" errors
	}
	return nil
}

// ftpFileInfo is a minimal os.FileInfo backed by FTP metadata.
type ftpFileInfo struct {
	name    string
	size    int64
	modTime time.Time
	isDir   bool
}

func (f *ftpFileInfo) Name() string      { return f.name }
func (f *ftpFileInfo) Size() int64       { return f.size }
func (f *ftpFileInfo) ModTime() time.Time { return f.modTime }
func (f *ftpFileInfo) IsDir() bool        { return f.isDir }
func (f *ftpFileInfo) Sys() any           { return nil }
func (f *ftpFileInfo) Mode() os.FileMode {
	if f.isDir {
		return os.ModeDir | 0o755
	}
	return 0o644
}
