package app

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
)

const (
	// maxDiffLoadWorkers caps concurrency for SFTP, where every worker shares
	// the single SFTP client. pkg/sftp pipelines concurrent requests over one
	// connection, so extra workers hide per-request round-trip latency — the
	// dominant cost when comparing many small files.
	maxDiffLoadWorkers = 8
	// maxFTPDiffLoadWorkers caps concurrency for FTP, where every worker beyond
	// the first opens its own connection. Connection setup is expensive and
	// servers commonly limit concurrent logins, so this stays low.
	maxFTPDiffLoadWorkers = 4
)

// diffLoadWorkers returns the worker count to use for a host's protocol.
func diffLoadWorkers(host config.Host) int {
	if isFTPProtocol(host.Protocol) {
		return maxFTPDiffLoadWorkers
	}
	return maxDiffLoadWorkers
}

type diffLoadItem struct {
	LocalPath  string
	RemotePath string
	Err        error
	Compare    bool
}

// compareFunc receives a job index plus the connection that worker should use.
type compareFunc func(idx int, conn remote.Client)

// forEachCompare runs fn for every index in jobs across a bounded worker pool.
// The first worker always reuses the connection that is already established, so
// a server allowing only one session per user still produces a complete diff.
// SFTP shares that one connection across all workers because pkg/sftp
// pipelines concurrent requests; for FTP every additional worker needs its own
// connection. A refused extra login lowers parallelism, but a terminal failure
// on any established connection fails the comparison. fn must only write to data
// owned by its idx, making the pool race-free without locking. prog may be nil.
func forEachCompare(ctx context.Context, host config.Host, conn remote.Client, jobs []int, prog *progress.Tracker, trust *tlstrust.Manager, required *tlstrust.Challenge, fn compareFunc) error {
	var activity *loadActivity
	if tracked, ok := conn.(*loadClient); ok {
		activity = tracked.activity
		ctx = activity.ctx
	}
	if err := conn.Err(); err != nil {
		return err
	}
	if len(jobs) == 0 {
		return nil
	}
	workerCount := minInt(diffLoadWorkers(host), len(jobs))
	if workerCount < 1 {
		workerCount = 1
	}

	jobCh := make(chan int)
	var wg sync.WaitGroup
	var failureMu sync.Mutex
	var failure error
	recordFailure := func(err error) {
		failureMu.Lock()
		defer failureMu.Unlock()
		if failure == nil {
			failure = err
		}
	}
	hasFailure := func() bool {
		failureMu.Lock()
		defer failureMu.Unlock()
		return failure != nil
	}
	work := func(workerConn remote.Client) {
		for idx := range jobCh {
			recordFailure(conn.Err())
			recordFailure(workerConn.Err())
			if ctx.Err() != nil || hasFailure() {
				continue // drain jobs so the producer can always finish
			}
			fn(idx, workerConn)
			if activity != nil {
				activity.touch()
			}
			recordFailure(workerConn.Err())
			if prog != nil {
				prog.Inc()
			}
		}
		recordFailure(workerConn.Err())
	}

	wg.Add(1)
	go func() {
		defer wg.Done()
		work(conn)
	}()

	for i := 1; i < workerCount; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if !isFTPProtocol(host.Protocol) {
				work(conn)
				return
			}
			recordFailure(conn.Err())
			if ctx.Err() != nil || hasFailure() {
				return
			}
			connectCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
			defer cancel()
			extraConn, err := remote.Connect(connectCtx, host, trust, required)
			if err != nil {
				var verificationErr *tlstrust.VerificationError
				if errors.As(err, &verificationErr) {
					recordFailure(err)
				}
				log.Debug("extra diff worker connect failed, reducing parallelism",
					"host", host.Name, "hostname", host.Hostname, "err", err)
				return
			}
			// Cancellation must also interrupt an extra worker's active I/O;
			// closing only the model's primary connection cannot release it.
			stopClose := context.AfterFunc(ctx, func() { _ = extraConn.Close() })
			defer func() {
				stopClose()
				_ = extraConn.Close()
				recordFailure(extraConn.Err())
			}()
			if activity != nil {
				work(&loadClient{Client: extraConn, activity: activity})
			} else {
				work(extraConn)
			}
		}()
	}

	for _, idx := range jobs {
		recordFailure(conn.Err())
		if ctx.Err() != nil || hasFailure() {
			break
		}
		jobCh <- idx
	}
	close(jobCh)
	wg.Wait()
	if err := conn.Err(); err != nil {
		return err
	}
	if failure != nil {
		return failure
	}
	return context.Cause(ctx)
}

func loadDiffItems(ctx context.Context, root *fs.Root, host config.Host, conn remote.Client, items []diffLoadItem, prog *progress.Tracker, trust *tlstrust.Manager, required *tlstrust.Challenge) ([]diff.Session, error) {
	results := make([]*diff.Session, len(items))
	var jobs []int
	for i, item := range items {
		if item.Compare {
			jobs = append(jobs, i)
			continue
		}
		if item.Err != nil {
			var verificationErr *tlstrust.VerificationError
			if errors.As(item.Err, &verificationErr) {
				return nil, fmt.Errorf("scan selected paths: %w", item.Err)
			}
			results[i] = &diff.Session{
				LocalPath:  item.LocalPath,
				RemotePath: item.RemotePath,
				Err:        item.Err,
				Loaded:     true,
			}
		}
	}

	prog.Set("Comparing files…", 0, len(jobs), len(jobs) == 0)
	securityErr := forEachCompare(ctx, host, conn, jobs, prog, trust, required, func(idx int, workerConn remote.Client) {
		item := items[idx]
		var activity func() error
		if tracked, ok := workerConn.(*loadClient); ok {
			activity = tracked.activity.checkpoint
		}
		result, diffErr := diff.CompareWithActivity(root, item.LocalPath, item.RemotePath, workerConn, activity)
		if diffErr != nil {
			log.Error("diff compare failed", "local", item.LocalPath, "remote", item.RemotePath, "err", diffErr)
		}
		if diffErr == nil && result != nil && !result.HasDiff() {
			return // identical — skip
		}
		results[idx] = &diff.Session{
			LocalPath:  item.LocalPath,
			RemotePath: item.RemotePath,
			Result:     result,
			Err:        diffErr,
			Loaded:     true,
		}
	})
	if securityErr != nil {
		return nil, securityErr
	}
	for _, session := range results {
		if session == nil || session.Err == nil {
			continue
		}
		var verificationErr *tlstrust.VerificationError
		if errors.As(session.Err, &verificationErr) {
			return nil, session.Err
		}
	}
	return sessionsFromResults(results), nil
}

func sessionsFromResults(results []*diff.Session) []diff.Session {
	sessions := make([]diff.Session, 0, len(results))
	for _, result := range results {
		if result != nil {
			sessions = append(sessions, *result)
		}
	}
	return sessions
}

func isFTPProtocol(protocol string) bool {
	return protocol == "ftp" || protocol == "ftps"
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}
