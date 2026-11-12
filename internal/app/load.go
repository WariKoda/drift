// Package app runs the use cases behind the TUI screens, currently building
// the diff sessions for a selection. It knows nothing about Bubble Tea;
// screens wrap these functions in commands and messages.
package app

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/pathmap"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// LoadRequest describes one comparison between a selection and a host.
type LoadRequest struct {
	Host   config.Host
	Config *config.MergedConfig
	Local  *fs.SelectionState
	Remote *fs.SelectionState
	// Options controls recursive ignore handling for this comparison.
	Options syncpolicy.ScopeOptions
	// Conn is an optional established connection to Host. Load takes
	// ownership: it is part of the result on success and closed on failure.
	Conn  remote.Client
	Trust *tlstrust.Manager
	// Required is set only for the first retry after a certificate prompt.
	Required *tlstrust.Challenge
	// IdleTimeout cancels a comparison without progress; zero means 60 seconds.
	IdleTimeout time.Duration
}

// LoadResult is a finished comparison. The caller owns Conn and Root and
// closes them together once the sessions are no longer needed.
type LoadResult struct {
	Sessions []diff.Session
	Conn     remote.Client
	Root     *fs.Root
	Scope    syncpolicy.ScopeSummary
}

// Load connects to the host and compares every file in the selection.
// Marked directories are expanded recursively. Local selections also walk the
// mapped remote directory to catch remote-only files; remote selections do the
// inverse and walk the mapped local directory to catch local-only files.
// Failures of single files stay in their session; connection, cancellation,
// timeout and certificate failures end the load and close what it owns.
func Load(parent context.Context, req LoadRequest, prog *progress.Tracker) (LoadResult, error) {
	host, cfg, options := req.Host, req.Config, req.Options
	localSel, remoteSel := req.Local, req.Remote
	existingConn, trust, required := req.Conn, req.Trust, req.Required
	idleTimeout := req.IdleTimeout
	if idleTimeout == 0 {
		idleTimeout = defaultIdleTimeout
	}
	activity := newLoadActivity(parent, idleTimeout)
	defer activity.cancel(nil)
	defer activity.finish(false)
	ctx := activity.ctx
	if existingConn != nil {
		activity.own(existingConn)
	}

	root, err := fs.OpenRoot(cfg.ProjectRoot)
	if err != nil {
		log.Error("open project root failed", "root", cfg.ProjectRoot, "err", err)
		return LoadResult{}, err
	}

	conn := existingConn
	abort := func(err error) error {
		if conn != nil {
			if terminalErr := conn.Err(); terminalErr != nil {
				err = terminalErr
			}
		}
		if cause := context.Cause(ctx); cause != nil {
			err = cause
		}
		_ = root.Close()
		return err
	}
	if conn == nil {
		prog.Set("Connecting…", 0, 0, true)
		connectCtx, cancelConnect := context.WithTimeout(ctx, 30*time.Second)
		conn, err = remote.Connect(connectCtx, host, trust, required)
		cancelConnect()
		if err != nil {
			log.Error("remote connect failed", "hostname", host.Hostname, "err", err)
			return LoadResult{}, abort(fmt.Errorf("connect to %s: %w", host.Hostname, err))
		}
		activity.own(conn)
		log.Info("remote connect", "host", host.Name, "hostname", host.Hostname)
	}
	primary := conn
	conn = &loadClient{Client: conn, activity: activity}
	activity.touch()
	if err := conn.Err(); err != nil {
		return LoadResult{}, abort(err)
	}
	if err := ctx.Err(); err != nil {
		return LoadResult{}, abort(err)
	}

	prog.Set("Scanning selections…", 0, 0, true)
	mapper := pathmap.New(cfg.ProjectRoot, cfg.Mappings, host)
	classifier, classifyErr := fs.NewClassifier(ctx, cfg.ProjectRoot)
	if classifyErr != nil {
		return LoadResult{}, abort(classifyErr)
	}
	var items []diffLoadItem
	var scope syncpolicy.ScopeSummary
	type candidatePair struct {
		local  string
		remote string
	}
	var candidatePairs []candidatePair
	var remoteWalks []candidatePair
	seenPairs := map[string]struct{}{}
	seenSkipped := map[string]struct{}{}
	explicitFiles := map[string]struct{}{}

	// Collect direct file selections before expanding any selected directory.
	// This makes explicit ignore exceptions independent of selection order.
	for _, localPath := range sortedMarkedPaths(localSel) {
		if info, statErr := root.Stat(localPath); statErr == nil && !info.IsDir() {
			explicitFiles[localPath] = struct{}{}
		}
	}
	for _, remotePath := range sortedMarkedPaths(remoteSel) {
		localPath, mapErr := mapper.RemoteToLocal(remotePath)
		if mapErr != nil {
			continue
		}
		if info, statErr := conn.Stat(remotePath); statErr == nil && !info.IsDir() {
			explicitFiles[localPath] = struct{}{}
		}
	}

	addError := func(localPath, remotePath string, err error) {
		items = append(items, diffLoadItem{LocalPath: localPath, RemotePath: remotePath, Err: err})
	}

	countSkipped := func(localPath string, class fs.PathClass, isDir bool) {
		key := localPath + fmt.Sprintf("\x00%t", isDir)
		if _, seen := seenSkipped[key]; seen {
			return
		}
		seenSkipped[key] = struct{}{}
		if class.HardExcluded {
			scope.HardExcludedSkipped++
		} else if isDir {
			scope.IgnoredDirsSkipped++
		} else {
			scope.IgnoredFilesSkipped++
		}
	}

	// Candidate pairs are classified together after both walks. This keeps
	// remote-only paths on the same batched Git invocation as local paths.
	addFile := func(localPath, remotePath string) {
		key := localPath + "\x00" + remotePath
		if _, seen := seenPairs[key]; seen {
			return
		}
		seenPairs[key] = struct{}{}
		candidatePairs = append(candidatePairs, candidatePair{local: localPath, remote: remotePath})
	}

	for _, localPath := range sortedMarkedPaths(localSel) {
		if err := conn.Err(); err != nil {
			return LoadResult{}, abort(err)
		}
		if err := ctx.Err(); err != nil {
			return LoadResult{}, abort(err)
		}
		info, statErr := root.Stat(localPath)
		activity.touch()
		if statErr != nil {
			addError(localPath, "", statErr)
			continue
		}
		classes, classErr := classifier.ClassifyBatch(ctx, []fs.ClassifyCandidate{{Path: localPath, IsDir: info.IsDir()}})
		if classErr != nil {
			return LoadResult{}, abort(classErr)
		}
		class := classes[0]
		if class.HardExcluded || (info.IsDir() && class.Ignored && !options.IncludeIgnored) {
			countSkipped(localPath, class, info.IsDir())
			continue
		}

		if !info.IsDir() {
			// ── Single local file ─────────────────────────────────
			remotePath, mapErr := mapper.LocalToRemote(localPath)
			if mapErr != nil {
				addError(localPath, "", mapErr)
				continue
			}
			addFile(localPath, remotePath)
			continue
		}

		// ── Directory: walk local side first ─────────────────────
		if walkErr := activity.walkLocalScope(localPath, classifier, options.IncludeIgnored, func(path string, class fs.PathClass) {
			countSkipped(path, class, true)
		}, func(p string) error {
			remotePath, mapErr := mapper.LocalToRemote(p)
			if mapErr != nil {
				addError(p, "", mapErr)
				return nil
			}
			addFile(p, remotePath)
			return nil
		}); walkErr != nil {
			addError(localPath, "", fmt.Errorf("walk local: %w", walkErr))
		}

		remoteDir, mapErr := mapper.LocalToRemote(localPath)
		if mapErr == nil {
			remoteWalks = append(remoteWalks, candidatePair{local: localPath, remote: remoteDir})
			continue
		}

		// An ancestor of a mapping is selectable but cannot itself be mapped.
		// Search only the effective mapping roots contained in that selection.
		mappings := cfg.Mappings
		if len(host.Mappings) > 0 {
			mappings = host.Mappings
		}
		for _, mapping := range mappings {
			localRoot := filepath.Join(cfg.ProjectRoot, filepath.FromSlash(mapping.Local))
			relative, relErr := filepath.Rel(localPath, localRoot)
			if relErr != nil || !filepath.IsLocal(relative) {
				continue
			}
			remoteRoot, rootErr := mapper.LocalToRemote(localRoot)
			if rootErr != nil {
				addError(localRoot, "", rootErr)
				continue
			}
			remoteWalks = append(remoteWalks, candidatePair{local: localRoot, remote: remoteRoot})
		}
	}

	// Ancestors sort before descendants. Mapping validation guarantees that
	// nested local roots mirror the same remote subtree.
	sort.Slice(remoteWalks, func(i, j int) bool { return remoteWalks[i].local < remoteWalks[j].local })
	var walkedRoots []string
	for _, walk := range remoteWalks {
		if err := ctx.Err(); err != nil {
			return LoadResult{}, abort(err)
		}
		covered := false
		for _, previous := range walkedRoots {
			if relative, err := filepath.Rel(previous, walk.local); err == nil && filepath.IsLocal(relative) {
				covered = true
				break
			}
		}
		if covered {
			continue
		}
		walkedRoots = append(walkedRoots, walk.local)
		if walkErr := conn.WalkFiles(walk.remote, func(remotePath string) error {
			localFilePath, revErr := mapper.RemoteToLocal(remotePath)
			if revErr != nil {
				return nil
			}
			addFile(localFilePath, remotePath)
			return nil
		}); walkErr != nil {
			// A missing counterpart is expected for a local-only directory.
			// Verify the root itself so a failed descendant listing stays visible.
			if _, statErr := conn.Stat(walk.remote); !errors.Is(statErr, os.ErrNotExist) {
				addError(walk.local, walk.remote, fmt.Errorf("walk remote: %w", walkErr))
			}
		}
	}

	for _, remotePath := range sortedMarkedPaths(remoteSel) {
		if err := conn.Err(); err != nil {
			return LoadResult{}, abort(err)
		}
		if err := ctx.Err(); err != nil {
			return LoadResult{}, abort(err)
		}
		localPath, mapErr := mapper.RemoteToLocal(remotePath)
		if mapErr != nil {
			addError("", remotePath, mapErr)
			continue
		}

		info, statErr := conn.Stat(remotePath)
		if statErr != nil {
			addError(localPath, remotePath, statErr)
			continue
		}
		classes, classErr := classifier.ClassifyBatch(ctx, []fs.ClassifyCandidate{{Path: localPath, IsDir: info.IsDir()}})
		if classErr != nil {
			return LoadResult{}, abort(classErr)
		}
		class := classes[0]
		if class.HardExcluded || (info.IsDir() && class.Ignored && !options.IncludeIgnored) {
			countSkipped(localPath, class, info.IsDir())
			continue
		}

		if !info.IsDir() {
			// ── Single remote file ────────────────────────────────
			addFile(localPath, remotePath)
			continue
		}

		// ── Directory: walk remote side first ───────────────────
		seenRemote := map[string]struct{}{}
		if walkErr := conn.WalkFiles(remotePath, func(p string) error {
			seenRemote[p] = struct{}{}
			localFilePath, revErr := mapper.RemoteToLocal(p)
			if revErr != nil {
				addError("", p, revErr)
				return nil
			}
			addFile(localFilePath, p)
			return nil
		}); walkErr != nil {
			addError(localPath, remotePath, fmt.Errorf("walk remote: %w", walkErr))
		}

		// ── Walk local side to catch local-only files ────────────
		localInfo, localErr := root.Stat(localPath)
		if localErr != nil {
			if !errors.Is(localErr, os.ErrNotExist) {
				addError(localPath, remotePath, localErr)
			}
			continue
		}
		if !localInfo.IsDir() {
			continue
		}
		if walkErr := activity.walkLocalScope(localPath, classifier, options.IncludeIgnored, func(path string, class fs.PathClass) {
			countSkipped(path, class, true)
		}, func(p string) error {
			remoteFilePath, revErr := mapper.LocalToRemote(p)
			if revErr != nil {
				addError(p, "", revErr)
				return nil
			}
			if _, seen := seenRemote[remoteFilePath]; seen {
				return nil // already covered by remote walk
			}
			addFile(p, remoteFilePath)
			return nil
		}); walkErr != nil {
			addError(localPath, remotePath, fmt.Errorf("walk local: %w", walkErr))
		}
	}

	if err := ctx.Err(); err != nil {
		return LoadResult{}, abort(err)
	}
	classifyCandidates := make([]fs.ClassifyCandidate, len(candidatePairs))
	for i, pair := range candidatePairs {
		classifyCandidates[i] = fs.ClassifyCandidate{Path: pair.local}
	}
	classes, classifyErr := classifier.ClassifyBatch(ctx, classifyCandidates)
	if classifyErr != nil {
		return LoadResult{}, abort(classifyErr)
	}
	for i, pair := range candidatePairs {
		class := classes[i]
		_, explicit := explicitFiles[pair.local]
		if class.HardExcluded || (class.Ignored && !options.IncludeIgnored && !explicit) {
			countSkipped(pair.local, class, false)
			continue
		}
		scope.Pairs++
		if class.Hidden || remotePathHidden(pair.remote, host.RootPath) {
			scope.Hidden++
		}
		if class.Ignored && explicit && !options.IncludeIgnored {
			scope.ExplicitIgnoredIncluded++
		}
		items = append(items, diffLoadItem{LocalPath: pair.local, RemotePath: pair.remote, Compare: true})
	}

	sessions, securityErr := loadDiffItems(ctx, root, host, conn, items, prog, trust, required)
	if securityErr != nil {
		return LoadResult{}, abort(securityErr)
	}
	if err := ctx.Err(); err != nil {
		return LoadResult{}, abort(err)
	}

	if err := conn.Err(); err != nil {
		return LoadResult{}, abort(err)
	}
	if err := activity.finish(true); err != nil {
		return LoadResult{}, abort(err)
	}
	return LoadResult{
		Sessions: sessions,
		Conn:     primary,
		Root:     root,
		Scope:    scope,
	}, nil
}
