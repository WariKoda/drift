// Package remoteedit manages private working copies for external remote editors.
package remoteedit

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"errors"
	"fmt"
	"io"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/pathmap"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	internalsync "github.com/WariKoda/drift/internal/sync"
	"github.com/WariKoda/drift/internal/tlstrust"
)

const MaxBytes = 10 << 20

var (
	ErrTooLarge = errors.New("file exceeds the 10 MiB remote editing limit")
	ErrConflict = errors.New("remote file changed; keep your copy or reload the remote file")
)

type Request struct {
	Host        config.Host
	RemotePath  string
	ProjectRoot string
	Mappings    []config.Mapping
}

type Result struct {
	File    string
	Changed bool
	Saved   bool
	Unknown bool
	Info    os.FileInfo
	Err     error
}

// Session serializes filesystem work and shutdown. The TUI owns active transports;
// a reconnect transport is held here only until its result is accepted.
type Session struct {
	mu        sync.Mutex
	request   Request
	dir       string
	file      string
	root      *fs.Root
	cacheRoot *os.Root
	original  [sha256.Size]byte
	loaded    bool
	candidate []byte
	snapshot  string
	pending   remote.Client
	closed    bool
}

func New(request Request) *Session {
	request.Mappings = append([]config.Mapping(nil), request.Mappings...)
	request.Host.Mappings = append([]config.Mapping(nil), request.Host.Mappings...)
	return &Session{request: request}
}

func (s *Session) Prepare(ctx context.Context, conn remote.Client) Result {
	s.mu.Lock()
	defer s.mu.Unlock()
	err := s.prepare(ctx, conn)
	return Result{File: s.file, Err: s.report("prepare", err)}
}

func (s *Session) prepare(ctx context.Context, conn remote.Client) error {
	if s.closed {
		return errors.New("edit session is closed")
	}
	if err := s.checkScope(ctx); err != nil {
		return err
	}
	if err := connectionError(ctx, conn); err != nil {
		return err
	}
	detach := remote.CloseOnContextDone(ctx, conn)
	defer detach()
	if _, err := s.remoteInfo(conn); err != nil {
		return err
	}
	if s.root == nil {
		cache, err := os.UserCacheDir()
		if err != nil {
			return err
		}
		if !filepath.IsAbs(cache) {
			return errors.New("the user cache directory must be absolute")
		}
		// Resolve the existing cache root before creating anything beneath it.
		// A cache inside the project would violate remote-only editing.
		resolvedCache, err := resolvePath(cache)
		if err != nil {
			return err
		}
		projectRoot, err := resolvePath(s.request.ProjectRoot)
		if err != nil {
			return err
		}
		base := filepath.Join(resolvedCache, "drift", "remote-edit")
		resolvedBase, err := resolvePath(base)
		if err != nil {
			return err
		}
		if rel, err := filepath.Rel(projectRoot, resolvedBase); err != nil || (rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))) {
			return errors.New("remote editing needs a cache directory outside the project")
		}
		if err := os.MkdirAll(resolvedBase, 0o700); err != nil {
			return err
		}
		s.cacheRoot, err = os.OpenRoot(resolvedBase)
		if err != nil {
			return err
		}
		name := "session-" + rand.Text()
		if err := s.cacheRoot.Mkdir(name, 0o700); err != nil {
			return err
		}
		s.dir = filepath.Join(resolvedBase, name)
		s.root, err = fs.OpenRoot(s.dir)
		if err != nil {
			return err
		}
		s.file = filepath.Join(s.dir, path.Base(s.request.RemotePath))
	}
	return s.download(ctx, conn)
}

func (s *Session) checkScope(ctx context.Context) error {
	name := path.Base(s.request.RemotePath)
	if name == "." || name == ".." || name == "/" || strings.ContainsAny(name, "\\\x00") {
		return errors.New("invalid remote filename")
	}
	mapper := pathmap.New(s.request.ProjectRoot, s.request.Mappings, s.request.Host)
	local, err := mapper.RemoteToLocal(s.request.RemotePath)
	if err != nil {
		return err
	}
	classifier, err := fs.NewClassifier(ctx, s.request.ProjectRoot)
	if err != nil {
		return err
	}
	classes, err := classifier.ClassifyBatch(ctx, []fs.ClassifyCandidate{{Path: local}})
	if err != nil {
		return err
	}
	if classes[0].HardExcluded {
		return fmt.Errorf("path is excluded: %s", classes[0].Reason)
	}
	return nil
}

func (s *Session) remoteInfo(conn remote.Client) (os.FileInfo, error) {
	entries, err := conn.ReadDir(path.Dir(s.request.RemotePath))
	if err != nil {
		return nil, fmt.Errorf("check remote file type: %w", err)
	}
	found := false
	for _, entry := range entries {
		if entry.Name != path.Base(s.request.RemotePath) {
			continue
		}
		if entry.Kind != fs.EntryFile || !entry.Mode.IsRegular() {
			return nil, fmt.Errorf("%w: target is not a regular file", ErrConflict)
		}
		found = true
		break
	}
	if !found {
		return nil, fmt.Errorf("%w: file is absent from the remote directory listing", ErrConflict)
	}
	info, err := conn.Stat(s.request.RemotePath)
	if err != nil {
		return nil, fmt.Errorf("stat remote file: %w", err)
	}
	if !info.Mode().IsRegular() {
		return nil, fmt.Errorf("%w: target is not a regular file", ErrConflict)
	}
	if info.Size() > MaxBytes {
		return nil, ErrTooLarge
	}
	return info, nil
}

func (s *Session) download(ctx context.Context, conn remote.Client) error {
	src, err := conn.Open(s.request.RemotePath)
	if err != nil {
		return err
	}
	if err := s.root.WriteAtomic(s.file, &limitedSource{ReadCloser: src}); err != nil {
		return err
	}
	data, err := s.readLocal(s.file)
	if err != nil {
		return err
	}
	if err := connectionError(ctx, conn); err != nil {
		return err
	}
	s.original = sha256.Sum256(data)
	s.loaded = true
	s.candidate = nil
	return nil
}

func (s *Session) Capture(ctx context.Context) Result {
	s.mu.Lock()
	defer s.mu.Unlock()
	result := Result{File: s.file}
	if err := ctx.Err(); err != nil {
		result.Err = err
	} else if s.closed || !s.loaded {
		result.Err = errors.New("no remote working copy loaded")
	} else {
		data, err := s.readLocal(s.file)
		if err == nil {
			if s.snapshot == "" {
				var name string
				name, err = fs.StagingName("editor-upload")
				s.snapshot = filepath.Join(s.dir, name)
			}
			if err == nil {
				err = s.root.WriteAtomic(s.snapshot, io.NopCloser(bytes.NewReader(data)))
			}
			if err == nil {
				var file *os.File
				file, err = s.root.Open(s.snapshot)
				if err == nil {
					err = errors.Join(file.Chmod(0o600), file.Close())
				}
			}
		}
		if err == nil {
			s.candidate = data
			result.Changed = sha256.Sum256(data) != s.original
		}
		result.Err = err
	}
	result.Err = s.report("capture", result.Err)
	return result
}

func (s *Session) readLocal(filePath string) ([]byte, error) {
	file, err := s.root.Open(filePath)
	if err != nil {
		return nil, err
	}
	info, statErr := file.Stat()
	if statErr != nil || !info.Mode().IsRegular() {
		return nil, errors.Join(statErr, errors.New("working copy is not a regular file"), file.Close())
	}
	if info.Size() > MaxBytes {
		return nil, errors.Join(ErrTooLarge, file.Close())
	}
	modeErr := file.Chmod(0o600)
	data, readErr := readBounded(file)
	return data, errors.Join(modeErr, readErr, file.Close())
}

// Review only compares. Reconnecting or accepting a certificate must never retry a write.
func (s *Session) Review(ctx context.Context, conn remote.Client) Result {
	s.mu.Lock()
	defer s.mu.Unlock()
	result := s.review(ctx, conn)
	result.Err = s.report("check", result.Err)
	return result
}

func (s *Session) review(ctx context.Context, conn remote.Client) Result {
	result := Result{File: s.file}
	if s.closed || !s.loaded {
		result.Err = errors.New("no remote working copy loaded")
		return result
	}
	if err := s.checkScope(ctx); err != nil {
		result.Err = err
		return result
	}
	if err := connectionError(ctx, conn); err != nil {
		result.Err = err
		return result
	}
	detach := remote.CloseOnContextDone(ctx, conn)
	defer detach()
	info, err := s.remoteInfo(conn)
	if err != nil {
		result.Err = err
		return result
	}
	src, err := conn.Open(s.request.RemotePath)
	if err != nil {
		result.Err = err
		return result
	}
	data, readErr := readBounded(src)
	result.Err = errors.Join(readErr, src.Close(), connectionError(ctx, conn))
	if result.Err != nil {
		return result
	}
	result.Info = info
	if s.candidate != nil && bytes.Equal(data, s.candidate) {
		result.Saved = true
		s.original = sha256.Sum256(s.candidate)
		local, err := s.readLocal(s.file)
		result.Changed = err != nil || !bytes.Equal(local, s.candidate)
		result.Err = err
	} else if sha256.Sum256(data) != s.original {
		result.Err = ErrConflict
	}
	return result
}

func (s *Session) Save(ctx context.Context, conn remote.Client, tracker *progress.Tracker) Result {
	s.mu.Lock()
	defer s.mu.Unlock()
	result := s.review(ctx, conn)
	if result.Err != nil {
		result.Err = s.report("check before upload", result.Err)
		return result
	}
	if s.candidate == nil || s.snapshot == "" {
		result.Err = errors.New("finish editing before uploading")
		return result
	}
	if !result.Saved {
		snapshot, err := s.readLocal(s.snapshot)
		if err != nil || !bytes.Equal(snapshot, s.candidate) {
			result.Err = s.report("check upload snapshot", errors.Join(err, errors.New("upload snapshot changed; edit again before uploading")))
			return result
		}
		if err := connectionError(ctx, conn); err != nil {
			result.Err = s.report("upload", err)
			return result
		}
		detach := remote.CloseOnContextDone(ctx, conn)
		defer detach()
		run := internalsync.Run(ctx, conn, s.root, []internalsync.Item{{
			LocalPath: s.snapshot, RemotePath: s.request.RemotePath, Decision: internalsync.DecisionUpload,
		}}, tracker)
		for _, failure := range run.Failures {
			result.Err = errors.Join(result.Err, failure.Err)
		}
		result.Err = errors.Join(result.Err, run.Err)
		if len(run.Completed) != 1 || result.Err != nil {
			result.Unknown = true
			if result.Err == nil {
				result.Err = errors.New("upload was not confirmed")
			}
			result.Err = s.report("upload outcome unknown", result.Err)
			return result
		}
		result.Saved = true
		result.Info, result.Err = conn.Stat(s.request.RemotePath)
	}
	s.original = sha256.Sum256(s.candidate)
	data, err := s.readLocal(s.file)
	result.Changed = err != nil || !bytes.Equal(data, s.candidate)
	result.Err = s.report("after upload", errors.Join(result.Err, err))
	return result
}

func (s *Session) Reload(ctx context.Context, conn remote.Client) Result {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.root == nil {
		return Result{File: s.file, Err: errors.New("no working directory available")}
	}
	err := s.checkScope(ctx)
	if err == nil {
		err = connectionError(ctx, conn)
	}
	if err == nil {
		detach := remote.CloseOnContextDone(ctx, conn)
		_, err = s.remoteInfo(conn)
		if err == nil {
			err = s.download(ctx, conn)
		}
		detach()
	}
	return Result{File: s.file, Err: s.report("reload", err)}
}

func (s *Session) Reconnect(ctx context.Context, trust *tlstrust.Manager, required *tlstrust.Challenge) (remote.Client, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil, errors.New("edit session is closed")
	}
	if s.pending != nil {
		return nil, errors.New("a reconnect result is already awaiting acceptance")
	}
	conn, err := remote.Connect(ctx, s.request.Host, trust, required)
	s.pending = conn
	return conn, s.report("reconnect", err)
}

// TakeConnection transfers a completed reconnect out of the shutdown guard.
func (s *Session) TakeConnection(conn remote.Client) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.pending != conn || conn == nil {
		return false
	}
	s.pending = nil
	return true
}

// Complete checks for edits made after the last confirmed snapshot before cleanup.
func (s *Session) Complete() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return errors.New("edit session was closed; working data is retained")
	}
	if s.loaded {
		data, err := s.readLocal(s.file)
		if err != nil || sha256.Sum256(data) != s.original {
			return s.report("retain later edits", errors.Join(err,
				errors.New("working copy changed after confirmation; retained"), s.close(false)))
		}
	}
	return s.close(true)
}

// Close retains working data unless removal was explicitly authorized by the caller.
func (s *Session) Close(remove bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.close(remove)
}

func (s *Session) close(remove bool) error {
	s.closed = true
	err := s.root.Close()
	s.root = nil
	if s.pending != nil {
		err = errors.Join(err, s.pending.Close())
		s.pending = nil
	}
	if s.cacheRoot != nil {
		if remove && s.dir != "" {
			err = errors.Join(err, s.cacheRoot.RemoveAll(filepath.Base(s.dir)))
		}
		err = errors.Join(err, s.cacheRoot.Close())
		s.cacheRoot = nil
	}
	return s.report("close", err)
}

func (s *Session) report(operation string, err error) error {
	if err != nil {
		log.Error("remote editor failed", "op", operation, "host", s.request.Host.Name, "remote", s.request.RemotePath, "err", err)
	}
	return err
}

func connectionError(ctx context.Context, conn remote.Client) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if conn == nil {
		return errors.New("remote disconnected; reconnect before uploading")
	}
	if err := conn.Err(); err != nil {
		return err
	}
	select {
	case <-conn.Done():
		return errors.New("remote connection is closed")
	default:
		return nil
	}
}

func readBounded(reader io.Reader) ([]byte, error) {
	data, err := io.ReadAll(io.LimitReader(reader, MaxBytes+1))
	if len(data) > MaxBytes {
		return nil, errors.Join(ErrTooLarge, err)
	}
	return data, err
}

type limitedSource struct {
	io.ReadCloser
	read int64
}

func (r *limitedSource) Read(p []byte) (int, error) {
	if int64(len(p)) > MaxBytes+1-r.read {
		p = p[:MaxBytes+1-r.read]
	}
	n, err := r.ReadCloser.Read(p)
	r.read += int64(n)
	if r.read > MaxBytes {
		return n, errors.Join(ErrTooLarge, err)
	}
	return n, err
}

// resolvePath also resolves symlinked ancestors of a not-yet-created cache path.
func resolvePath(value string) (string, error) {
	absolute, err := filepath.Abs(value)
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(absolute)
	if errors.Is(err, os.ErrNotExist) {
		parent, parentErr := resolvePath(filepath.Dir(absolute))
		if parentErr != nil {
			return "", parentErr
		}
		return filepath.Join(parent, filepath.Base(absolute)), nil
	}
	return resolved, err
}
