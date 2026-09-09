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
	"net"
	"os"
	"path"
	"sort"
	"strings"
	"sync"
	"time"

	ftplib "github.com/jlaffaye/ftp"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// Client wraps an FTP connection.
type Client struct {
	conn   *ftplib.ServerConn
	opMu   sync.Mutex
	Host   config.Host
	policy tlstrust.Policy

	// opMu protects the library connection and lastActivity. lifeMu never
	// waits for opMu, so shutdown can interrupt a blocked command or stream.
	lastActivity time.Time
	lifeMu       sync.Mutex
	closed       bool
	err          error
	done         chan struct{}
	monitorDone  chan struct{}
	opReleased   chan struct{}
	lifeCtx      context.Context
	cancel       context.CancelFunc
	transports   map[*trackedConn]struct{}
	children     map[*Client]struct{}
	control      net.Conn
	closeOnce    sync.Once
	dialWG       sync.WaitGroup
}

// Connect dials an FTP server and logs in.
func Connect(ctx context.Context, host config.Host, policy tlstrust.Policy) (*Client, error) {
	return connect(ctx, host, policy, host.KeepAliveDuration(), config.KeepAliveTimeout)
}

// connect accepts short probe timings for loopback tests, not user options.
func connect(ctx context.Context, host config.Host, policy tlstrust.Policy, interval, probeTimeout time.Duration) (*Client, error) {
	port := host.Port
	if port == 0 {
		port = 21
	}
	addr := net.JoinHostPort(strings.Trim(host.Hostname, "[]"), fmt.Sprintf("%d", port))

	var tlsConfig *tls.Config
	opts := []ftplib.DialOption{}
	if host.Protocol == "ftps" {
		endpoint, err := tlstrust.NormalizeEndpoint(host.Protocol, host.Hostname, port)
		if err != nil {
			return nil, err
		}
		// TLS 1.2 remains pinned because some FTP servers complete a TLS 1.3
		// control handshake but abort larger data transfers with status 426.
		tlsConfig = policy.TLSConfig(endpoint)
		opts = append(opts, ftplib.DialWithExplicitTLS(tlsConfig))
	}

	lifeCtx, cancel := context.WithCancel(context.Background())
	c := &Client{
		Host: host, policy: policy, lifeCtx: lifeCtx, cancel: cancel,
		done: make(chan struct{}), monitorDone: make(chan struct{}), opReleased: make(chan struct{}),
		transports: make(map[*trackedConn]struct{}), children: make(map[*Client]struct{}),
	}
	// The setup context also interrupts greeting, TLS and login I/O. It is
	// detached before the monitor starts; data dials use the client lifetime.
	setupCtx, setupCancel := context.WithTimeout(ctx, ftpConnectTimeout)
	defer setupCancel()
	stopSetup := context.AfterFunc(setupCtx, func() { c.terminate(nil) })
	defer stopSetup()
	defer func() {
		if c.conn == nil {
			c.terminate(nil)
		}
	}()
	firstDial := true // the library invokes its dial function serially
	opts = append(opts, ftplib.DialWithDialFunc(func(network, address string) (net.Conn, error) {
		control := firstDial
		firstDial = false
		raw, err := c.dial(network, address)
		if err != nil {
			return nil, err
		}
		if control {
			c.control = raw
			deadline, _ := setupCtx.Deadline()
			if err := raw.SetDeadline(deadline); err != nil {
				return nil, err
			}
			return raw, nil
		}
		// DialWithDialFunc bypasses the library's data TLS wrapper. Keep
		// the handshake lazy, as FTP servers may wait for RETR/STOR first.
		if tlsConfig != nil {
			return tls.Client(raw, tlsConfig), nil
		}
		return raw, nil
	}))
	conn, err := ftplib.Dial(addr, opts...)
	if err != nil {
		return nil, fmt.Errorf("connect to %s: %w", addr, err)
	}
	pass := os.ExpandEnv(host.Auth.Password)
	if err := conn.Login(host.User, pass); err != nil {
		return nil, fmt.Errorf("login to %s: %w", addr, err)
	}
	if !stopSetup() || setupCtx.Err() != nil {
		return nil, fmt.Errorf("connect to %s: %w", addr, setupCtx.Err())
	}
	if err := c.control.SetDeadline(time.Time{}); err != nil {
		return nil, fmt.Errorf("clear FTP setup deadline: %w", err)
	}
	c.conn = conn
	c.lastActivity = time.Now()
	if interval > 0 {
		go c.monitor(interval, probeTimeout)
	} else {
		close(c.monitorDone)
	}
	return c, nil
}

// Stat returns file info for a remote path.
//
// MLST is the only reliable way to tell a directory from a file here. SIZE
// cannot do it: many servers (vsftpd and ProFTPD among them) answer it for
// directories with the allocation size instead of refusing, so a successful
// SIZE says nothing about the entry type. A directory that Stat reports as a
// file reaches diff.Compare as a file pair, where reading it fails with
// "is a directory".
//
// MLST also carries size and modification time, so one control-connection
// command replaces SIZE plus MDTM, and its timestamps have second precision.
// Servers without MLST fall back to SIZE and keep that ambiguity.
func (c *Client) Stat(remotePath string) (info os.FileInfo, err error) {
	if err = c.beginOperation(); err != nil {
		return nil, err
	}
	defer func() { c.endOperation(err) }()

	if entry, err := c.conn.GetEntry(remotePath); err == nil {
		return &ftpFileInfo{
			name:    path.Base(remotePath),
			size:    int64(entry.Size),
			modTime: entry.Time,
			isDir:   entry.Type == ftplib.EntryTypeFolder,
		}, nil
	}

	size, err := c.conn.FileSize(remotePath)
	if err != nil {
		// Check if it's a directory by attempting to list it.
		entries, listErr := c.conn.List(remotePath)
		if listErr != nil {
			return nil, errors.Join(err, listErr)
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
func (c *Client) ReadDir(remotePath string) (entries []*fs.FileEntry, err error) {
	if err = c.beginOperation(); err != nil {
		return nil, err
	}
	defer func() { c.endOperation(err) }()

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
	if err := c.beginOperation(); err != nil {
		return nil, err
	}
	r, err := c.conn.Retr(remotePath)
	if err != nil {
		c.endOperation(err)
		return nil, fmt.Errorf("retr %s: %w", remotePath, err)
	}
	return &lockedReadCloser{
		ReadCloser: r,
		unlock:     c.endOperation,
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
func (c *Client) Upload(remotePath string, src io.Reader) (err error) {
	if err = c.beginOperation(); err != nil {
		return err
	}
	defer func() { c.endOperation(err) }()

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
	return c.WalkFilesWithActivity(remoteRoot, fn, nil)
}

// WalkFilesWithActivity reports completed listings, even for empty directories.
func (c *Client) WalkFilesWithActivity(remoteRoot string, fn func(string) error, activity func() error) error {
	if err := c.connectionError(); err != nil {
		return err
	}
	return c.parallelWalkFiles(remoteRoot, fn, activity)
}

func (c *Client) parallelWalkFiles(remoteRoot string, fn func(string) error, activity func() error) (err error) {
	workers := []*Client{c}
	defer func() {
		for _, worker := range workers[1:] {
			_ = worker.Close()
			// Join the monitor before inspecting its final status. A probe
			// can fail between the last listing and worker cleanup.
			if err == nil {
				err = worker.Err()
			}
			c.lifeMu.Lock()
			delete(c.children, worker)
			c.lifeMu.Unlock()
		}
	}()
	for len(workers) < maxWalkWorkers {
		// Close also joins worker setups that have not yet produced a
		// client to attach. Cancellation alone would only stop them later.
		c.lifeMu.Lock()
		if c.closed {
			c.lifeMu.Unlock()
			return c.connectionError()
		}
		c.dialWG.Add(1)
		c.lifeMu.Unlock()
		ctx, cancel := context.WithTimeout(c.lifeCtx, 30*time.Second)
		worker, err := Connect(ctx, c.Host, c.policy)
		cancel()
		if err != nil {
			c.dialWG.Done()
			var verificationErr *tlstrust.VerificationError
			if errors.As(err, &verificationErr) {
				return fmt.Errorf("connect FTP walk worker: %w", err)
			}
			if terminalErr := c.connectionError(); terminalErr != nil {
				return terminalErr
			}
			log.Debug("FTP walk using fewer connections", "host", c.Host.Name, "workers", len(workers), "err", err)
			break
		}
		// Attach under the lifecycle lock so Close cannot miss a worker
		// which finishes connecting concurrently with shutdown.
		c.lifeMu.Lock()
		closed := c.closed
		if !closed {
			c.children[worker] = struct{}{}
		}
		c.lifeMu.Unlock()
		if closed {
			_ = worker.Close()
			c.dialWG.Done()
			return c.connectionError()
		}
		workers = append(workers, worker)
		c.dialWG.Done()
		if activity != nil {
			if err := activity(); err != nil {
				return err
			}
		}
	}

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
		if activity != nil {
			if err := activity(); err != nil {
				firstErr = err
				return
			}
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
			if activity != nil {
				if err := activity(); err != nil {
					recordErr(err)
					return nil
				}
			}
			dirs := worker.walkDirLevel(dir, handleFile, recordErr)
			if activity != nil {
				recordErr(activity())
			}
			return dirs
		})
	}
	walkQueue(remoteRoot, listers, shouldStop)
	for _, worker := range workers {
		if err := worker.connectionError(); err != nil {
			recordErr(err)
		}
	}
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
	if err := c.beginOperation(); err != nil {
		recordErr(err)
		return nil
	}
	entries, err := c.conn.List(dir)
	c.endOperation(err)
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
func (c *Client) DeleteFile(remotePath string) (err error) {
	if err = c.beginOperation(); err != nil {
		return err
	}
	defer func() { c.endOperation(err) }()
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
	unlock   func(error)
}

func (r *lockedReadCloser) Close() error {
	r.once.Do(func() {
		r.closeErr = r.ReadCloser.Close()
		r.unlock(r.closeErr)
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
