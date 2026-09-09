package diffview

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
)

// ErrDiffIdleTimeout distinguishes a stalled comparison from user cancellation.
var ErrDiffIdleTimeout = errors.New("diff comparison inactivity timeout")

const diffIdleTimeout = 60 * time.Second

// loadActivity serializes timeout, cancellation and successful ownership transfer.
// Close runs outside the lock, independently for each connection.
type loadActivity struct {
	mu         sync.Mutex
	ctx        context.Context
	cancel     context.CancelCauseFunc
	timer      *time.Timer
	stopCancel func() bool
	timeout    time.Duration
	last       time.Time
	finished   bool
	owned      map[io.Closer]struct{}
}

func newLoadActivity(parent context.Context, timeout time.Duration) *loadActivity {
	ctx, cancel := context.WithCancelCause(parent)
	a := &loadActivity{ctx: ctx, cancel: cancel, timeout: timeout, last: time.Now(), owned: make(map[io.Closer]struct{})}
	a.mu.Lock()
	a.timer = time.AfterFunc(timeout, a.expire)
	a.stopCancel = context.AfterFunc(ctx, func() { a.finish(false) })
	a.mu.Unlock()
	return a
}

func (a *loadActivity) touch() {
	a.mu.Lock()
	defer a.mu.Unlock()
	if !a.finished && a.ctx.Err() == nil {
		a.last = time.Now()
	}
}

func (a *loadActivity) checkpoint() error {
	if err := context.Cause(a.ctx); err != nil {
		return err
	}
	a.touch()
	return nil
}

func (a *loadActivity) expire() {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.finished || a.ctx.Err() != nil {
		return
	}
	if remaining := a.timeout - time.Since(a.last); remaining > 0 {
		a.timer.Reset(remaining)
		return
	}
	a.cancel(fmt.Errorf("%w: no progress for %s", ErrDiffIdleTimeout, a.timeout))
}

func (a *loadActivity) own(c io.Closer) {
	a.mu.Lock()
	rejected := a.finished || a.ctx.Err() != nil
	if !rejected {
		a.owned[c] = struct{}{}
	}
	a.mu.Unlock()
	if rejected {
		go closeLoadResource(c)
	}
}

func closeLoadResource(c io.Closer) {
	if err := c.Close(); err != nil {
		log.Error("close diff load connection failed", "err", err)
	}
}

// finish(true) releases resources only if neither cancellation nor timeout won.
func (a *loadActivity) finish(success bool) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	err := context.Cause(a.ctx)
	if a.finished {
		return err
	}
	a.finished = true
	a.timer.Stop()
	if a.stopCancel != nil {
		a.stopCancel()
	}
	if !success || err != nil {
		for c := range a.owned {
			go closeLoadResource(c)
		}
	}
	a.owned = nil
	return err
}

type loadClient struct {
	remote.Client
	activity *loadActivity
}

func (c *loadClient) Stat(path string) (os.FileInfo, error) {
	if err := context.Cause(c.activity.ctx); err != nil {
		return nil, err
	}
	info, err := c.Client.Stat(path)
	c.activity.touch()
	return info, err
}

func (c *loadClient) Open(path string) (io.ReadCloser, error) {
	if err := context.Cause(c.activity.ctx); err != nil {
		return nil, err
	}
	r, err := c.Client.Open(path)
	if err != nil {
		return nil, err
	}
	c.activity.touch()
	return &loadReader{ReadCloser: r, activity: c.activity}, nil
}

func (c *loadClient) ReadFile(path string) ([]byte, error) {
	r, err := c.Open(path)
	if err != nil {
		return nil, err
	}
	data, readErr := io.ReadAll(r)
	return data, errors.Join(readErr, r.Close())
}

type loadReader struct {
	io.ReadCloser
	activity *loadActivity
}

func (r *loadReader) Read(p []byte) (int, error) {
	if err := context.Cause(r.activity.ctx); err != nil {
		return 0, err
	}
	n, err := r.ReadCloser.Read(p)
	if n > 0 {
		r.activity.touch()
	}
	return n, err
}

// WalkFiles retains protocol-specific parallelism while observing empty directories.
func (c *loadClient) WalkFiles(root string, fn func(string) error) error {
	if err := context.Cause(c.activity.ctx); err != nil {
		return err
	}
	return c.Client.WalkFilesWithActivity(root, fn, c.activity.checkpoint)
}

func (a *loadActivity) walkLocal(root string, fn func(string) error) error {
	return filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if cause := context.Cause(a.ctx); cause != nil {
			return cause
		}
		a.touch()
		if err != nil {
			return nil // Match fs.WalkFiles' unreadable-entry policy.
		}
		if d.IsDir() {
			if path != root && fs.ShouldSkipDir(d.Name()) {
				return filepath.SkipDir
			}
			return nil
		}
		if d.Type()&os.ModeSymlink != 0 {
			return nil
		}
		return fn(path)
	})
}
