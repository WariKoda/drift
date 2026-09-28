package app

import (
	"context"
	"errors"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// RefreshRequest compares the files of existing sessions again over an open
// connection. Refresh borrows Conn and Root; the caller keeps ownership.
type RefreshRequest struct {
	Host     config.Host
	Conn     remote.Client
	Root     *fs.Root
	Sessions []diff.Session
	Trust    *tlstrust.Manager
}

// Refresh re-diffs all sessions in parallel using the worker pool. Order and
// count are preserved, identical files included, so the file list stays
// stable. The returned sessions are filled even when an error is returned:
// cancellation, a certificate failure or a terminal connection failure.
func Refresh(ctx context.Context, req RefreshRequest, prog *progress.Tracker) ([]diff.Session, error) {
	sessions := req.Sessions
	root := req.Root
	refreshed := append([]diff.Session(nil), sessions...)
	jobs := make([]int, len(sessions))
	for i := range sessions {
		jobs[i] = i
	}
	securityErr := forEachCompare(ctx, req.Host, req.Conn, jobs, prog, req.Trust, nil, func(idx int, workerConn remote.Client) {
		s := sessions[idx]
		result, err := diff.Compare(root, s.LocalPath, s.RemotePath, workerConn)
		if err != nil {
			log.Error("diff refresh failed", "local", s.LocalPath, "remote", s.RemotePath, "err", err)
		}
		refreshed[idx] = diff.Session{
			LocalPath:  s.LocalPath,
			RemotePath: s.RemotePath,
			Result:     result,
			Err:        err,
			Loaded:     true,
		}
	})
	if ctx.Err() != nil {
		return refreshed, context.Canceled
	}
	if securityErr != nil {
		return refreshed, securityErr
	}
	for _, session := range refreshed {
		var verificationErr *tlstrust.VerificationError
		if session.Err != nil && errors.As(session.Err, &verificationErr) {
			return refreshed, session.Err
		}
	}
	return refreshed, req.Conn.Err()
}
