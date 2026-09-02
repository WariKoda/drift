// Package ftp provides an FTP client that implements remote.Client.
package ftp

import (
	"context"
	"crypto/rand"
	"crypto/tls"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path"
	"sort"
	"strings"
	"sync"
	"time"

	ftplib "github.com/jlaffaye/ftp"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
)

// Client wraps an FTP connection.
type Client struct {
	conn *ftplib.ServerConn
	opMu sync.Mutex
	Host config.Host
}

// Connect dials an FTP server and logs in.
func Connect(ctx context.Context, host config.Host) (*Client, error) {
	port := host.Port
	if port == 0 {
		port = 21
	}
	addr := fmt.Sprintf("%s:%d", host.Hostname, port)

	opts := []ftplib.DialOption{
		ftplib.DialWithContext(ctx),
		ftplib.DialWithTimeout(15 * time.Second),
	}
	if host.Protocol == "ftps" {
		opts = append(opts, ftplib.DialWithExplicitTLS(&tls.Config{
			ServerName: host.Hostname,
			// Some FTP servers negotiate TLS 1.3 successfully on the control
			// connection but abort larger data transfers with status 426.
			MinVersion:         tls.VersionTLS12,
			MaxVersion:         tls.VersionTLS12,
			InsecureSkipVerify: host.InsecureTLS, //nolint:gosec // opt-in per host for self-signed certs
		}))
	}

	conn, err := ftplib.Dial(addr, opts...)
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
	c.opMu.Lock()
	defer c.opMu.Unlock()
	return c.conn.Quit()
}

// Stat returns file info for a remote path.
func (c *Client) Stat(remotePath string) (os.FileInfo, error) {
	c.opMu.Lock()
	defer c.opMu.Unlock()

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

// ReadDir reads one remote directory level.
// Directories are returned before files; both groups sorted alphabetically.
func (c *Client) ReadDir(remotePath string) ([]*fs.FileEntry, error) {
	c.opMu.Lock()
	defer c.opMu.Unlock()

	items, err := c.conn.List(remotePath)
	if err != nil {
		return nil, err
	}

	var dirs, files []*fs.FileEntry
	for _, item := range items {
		if item.Name == "." || item.Name == ".." {
			continue
		}
		kind := fs.EntryFile
		mode := os.FileMode(0o644)
		switch item.Type {
		case ftplib.EntryTypeFolder:
			kind = fs.EntryDir
			mode = os.ModeDir | 0o755
		case ftplib.EntryTypeLink:
			kind = fs.EntrySymlink
			mode = os.ModeSymlink | 0o777
		}
		entry := &fs.FileEntry{
			Name:    item.Name,
			Path:    path.Join(remotePath, item.Name),
			Kind:    kind,
			Size:    int64(item.Size),
			ModTime: item.Time,
			Mode:    mode,
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
	c.opMu.Lock()
	r, err := c.conn.Retr(remotePath)
	if err != nil {
		c.opMu.Unlock()
		return nil, fmt.Errorf("retr %s: %w", remotePath, err)
	}
	return &lockedReadCloser{
		ReadCloser: r,
		unlock:     c.opMu.Unlock,
	}, nil
}

// ReadFile reads the full contents of a remote file.
func (c *Client) ReadFile(remotePath string) ([]byte, error) {
	r, err := c.Open(remotePath)
	if err != nil {
		return nil, err
	}
	data, readErr := io.ReadAll(r)
	closeErr := r.Close()
	if err := errors.Join(readErr, closeErr); err != nil {
		return nil, err
	}
	return data, nil
}

// Upload atomically writes everything src yields to a remote path, creating
// parent directories as needed. The existing target is replaced only after the
// staged upload has completed successfully.
func (c *Client) Upload(remotePath string, src io.Reader) error {
	c.opMu.Lock()
	defer c.opMu.Unlock()

	if err := c.ensureDir(path.Dir(remotePath)); err != nil {
		return err
	}

	stageBase, err := stagingName(path.Base(remotePath))
	if err != nil {
		return err
	}
	stagePath := path.Join(path.Dir(remotePath), stageBase)
	committed := false
	defer func() {
		if !committed {
			_ = c.conn.Delete(stagePath)
		}
	}()

	if err := c.conn.Stor(stagePath, src); err != nil {
		return fmt.Errorf("stor staged file for %s: %w", remotePath, err)
	}
	if err := c.conn.Rename(stagePath, remotePath); err != nil {
		return fmt.Errorf("replace remote %s: %w", remotePath, err)
	}
	committed = true
	return nil
}

const maxWalkWorkers = 4

// WalkFiles calls fn for every regular file under remoteRoot, recursively.
func (c *Client) WalkFiles(remoteRoot string, fn func(string) error) error {
	c.opMu.Lock()
	defer c.opMu.Unlock()
	return c.parallelWalkFiles(remoteRoot, fn)
}

func (c *Client) parallelWalkFiles(remoteRoot string, fn func(string) error) error {
	workers := []*Client{c}
	for len(workers) < maxWalkWorkers {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		worker, err := Connect(ctx, c.Host)
		cancel()
		if err != nil {
			break
		}
		workers = append(workers, worker)
	}
	defer func() {
		for _, worker := range workers[1:] {
			_ = worker.Close()
		}
	}()

	var mu sync.Mutex
	var firstErr error

	// handleFile holds mu for the whole fn call: fn writes shared maps/slices
	// in the caller (LoadCmd), so the lock serializes those writes.
	handleFile := func(p string) {
		mu.Lock()
		defer mu.Unlock()
		if firstErr != nil {
			return
		}
		if err := fn(p); err != nil {
			firstErr = err
		}
	}
	// recordErr surfaces the first directory-listing failure. Without it a
	// transient LIST error — common on FTPS data connections — would silently
	// drop an entire subtree from the walk, so those files would never sync.
	recordErr := func(err error) {
		mu.Lock()
		defer mu.Unlock()
		if firstErr == nil {
			firstErr = err
		}
	}
	shouldStop := func() bool {
		mu.Lock()
		defer mu.Unlock()
		return firstErr != nil
	}

	listers := make([]func(string) []string, 0, len(workers))
	for _, worker := range workers {
		listers = append(listers, func(dir string) []string {
			return worker.walkDirLevel(dir, handleFile, recordErr)
		})
	}
	walkQueue(remoteRoot, listers, shouldStop)
	return firstErr
}

// walkQueue walks root breadth-first over len(listers) concurrent listers, one
// per connection. A lister reports the files it finds itself and returns the
// subdirectories it found; walkQueue schedules those.
//
// The pending directories live in this function, not in a channel the listers
// write to. That is the whole point: share one bounded channel between readers
// and writers and a wide enough tree fills the buffer, every lister blocks on
// its own send, and nobody is left to drain it. Here only walkQueue appends to
// the queue, so fan-out costs memory instead of the entire walk.
//
// walkQueue returns once every directory has been listed, or once stop reports
// that the caller has given up.
func walkQueue(root string, listers []func(string) []string, stop func() bool) {
	work := make(chan string)
	found := make(chan []string)

	var listerWG sync.WaitGroup
	for _, list := range listers {
		listerWG.Add(1)
		go func() {
			defer listerWG.Done()
			for dir := range work {
				found <- list(dir)
			}
		}()
	}

	queue := []string{root}
	next := 0    // index of the directory handed out next
	listing := 0 // directories currently in the hands of a lister
	for listing > 0 || (next < len(queue) && !stop()) {
		var out chan string
		var dir string
		if next < len(queue) && !stop() {
			dir, out = queue[next], work
		}
		// Offering work and collecting results in one select keeps this loop
		// ready to drain found even while every lister is busy.
		select {
		case out <- dir:
			next++
			listing++
		case subdirs := <-found:
			listing--
			queue = append(queue, subdirs...)
		}
	}

	// Reaching here means listing == 0, so no lister is stuck mid-send.
	close(work)
	listerWG.Wait()
}

// walkDirLevel lists one directory: files go to handleFile, subdirectories are
// returned for the caller to schedule.
func (c *Client) walkDirLevel(dir string, handleFile func(string), recordErr func(error)) []string {
	entries, err := c.conn.List(dir)
	if err != nil {
		recordErr(fmt.Errorf("list %s: %w", dir, err))
		return nil
	}
	var subdirs []string
	for _, e := range entries {
		if e.Name == "." || e.Name == ".." {
			continue
		}
		p := strings.TrimSuffix(dir, "/") + "/" + e.Name
		switch e.Type {
		case ftplib.EntryTypeFolder:
			if fs.ShouldSkipDir(e.Name) {
				continue
			}
			subdirs = append(subdirs, p)
		case ftplib.EntryTypeFile:
			handleFile(p)
		}
	}
	return subdirs
}

// DeleteFile removes a file on the remote host.
func (c *Client) DeleteFile(remotePath string) error {
	c.opMu.Lock()
	defer c.opMu.Unlock()
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

// stagingName returns an unpredictable hidden sibling name. Staging files must
// live beside their target so the final rename stays within one directory.
func stagingName(base string) (string, error) {
	var token [16]byte
	if _, err := rand.Read(token[:]); err != nil {
		return "", fmt.Errorf("generate staging name: %w", err)
	}
	return "." + base + ".drift-tmp-" + hex.EncodeToString(token[:]), nil
}

// lockedReadCloser keeps the FTP control connection reserved for the complete
// lifetime of a data transfer. FTP permits only one active data connection per
// control connection, so releasing the client lock when Retr returns would
// still allow another command to corrupt the in-flight transfer.
type lockedReadCloser struct {
	io.ReadCloser
	once     sync.Once
	closeErr error
	unlock   func()
}

func (r *lockedReadCloser) Close() error {
	r.once.Do(func() {
		r.closeErr = r.ReadCloser.Close()
		r.unlock()
	})
	return r.closeErr
}

// ftpFileInfo is a minimal os.FileInfo backed by FTP metadata.
type ftpFileInfo struct {
	name    string
	size    int64
	modTime time.Time
	isDir   bool
}

func (f *ftpFileInfo) Name() string       { return f.name }
func (f *ftpFileInfo) Size() int64        { return f.size }
func (f *ftpFileInfo) ModTime() time.Time { return f.modTime }
func (f *ftpFileInfo) IsDir() bool        { return f.isDir }
func (f *ftpFileInfo) Sys() any           { return nil }
func (f *ftpFileInfo) Mode() os.FileMode {
	if f.isDir {
		return os.ModeDir | 0o755
	}
	return 0o644
}
