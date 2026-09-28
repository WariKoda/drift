package sync

import (
	"context"
	"errors"
	"fmt"
	"net/textproto"
	"strings"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// Item is one planned operation on a local/remote file pair.
type Item struct {
	LocalPath  string
	RemotePath string
	Decision   Decision
}

// Failure describes one failed operation.
type Failure struct {
	Operation string
	Path      string
	Reason    string
	Err       error
}

// Result reports what a run confirmed. Completed and Failures index into the
// items passed to Run; items after a stop are neither.
type Result struct {
	Completed []int
	Failures  []Failure
	// Err is context.Canceled after cancellation or the terminal connection
	// failure that stopped the run; nil when every item was attempted.
	Err error
}

// Run executes items in order over conn and root. It stops before the next
// item once ctx is canceled or the connection has failed, and after a
// certificate failure. Nothing is retried: a write that failed on a dead
// connection may have reached the server, so its failure says so.
// prog counts every attempted or skipped item and may be nil.
func Run(ctx context.Context, conn remote.Client, root *fs.Root, items []Item, prog *progress.Tracker) Result {
	var result Result
	for i, item := range items {
		if ctx.Err() != nil || connectionError(conn) != nil {
			break
		}
		var err error
		var op string
		var failurePath string
		switch item.Decision {
		case DecisionUpload:
			op = "upload"
			failurePath = item.LocalPath
			err = uploadFile(conn, root, item.LocalPath, item.RemotePath)
		case DecisionDownload:
			op = "download"
			failurePath = item.RemotePath
			err = downloadFile(conn, root, item.RemotePath, item.LocalPath)
		case DecisionDeleteLocal:
			op = "delete local"
			failurePath = item.LocalPath
			err = root.Remove(item.LocalPath)
		case DecisionDeleteRemote:
			op = "delete remote"
			failurePath = item.RemotePath
			err = conn.DeleteFile(item.RemotePath)
		default:
			prog.Inc()
			continue
		}
		err = OperationError(conn, err)
		if ctx.Err() != nil {
			if err == nil {
				result.Completed = append(result.Completed, i)
				prog.Inc()
			}
			break
		}
		if err != nil {
			log.Error("sync file", "op", op, "local", item.LocalPath, "remote", item.RemotePath, "err", err)
			reason := strings.Join(strings.Fields(err.Error()), " ")
			var protocolErr *textproto.Error
			if conn.Err() == nil && errors.As(err, &protocolErr) {
				reason = protocolErr.Error()
			}
			result.Failures = append(result.Failures, Failure{
				Operation: op,
				Path:      failurePath,
				Reason:    reason,
				Err:       err,
			})
			var verificationErr *tlstrust.VerificationError
			if errors.As(err, &verificationErr) {
				prog.Inc()
				break
			}
		} else {
			log.Debug("sync file ok", "op", op, "local", item.LocalPath, "remote", item.RemotePath)
			result.Completed = append(result.Completed, i)
		}
		prog.Inc()
	}
	if ctx.Err() != nil {
		result.Err = context.Canceled
	} else {
		result.Err = connectionError(conn)
	}
	return result
}

// OperationError keeps the operation error and the terminal cause. A failed
// write may have reached the server, so retrying requires a fresh comparison.
func OperationError(conn remote.Client, err error) error {
	if err != nil && conn.Err() != nil {
		return fmt.Errorf("outcome unknown; compare again before syncing: %w", errors.Join(err, conn.Err()))
	}
	return err
}

// connectionError reports a terminal monitor failure or a closed connection.
func connectionError(conn remote.Client) error {
	if err := conn.Err(); err != nil {
		return err
	}
	select {
	case <-conn.Done():
		return errors.New("connection is closed")
	default:
		return nil
	}
}

// uploadFile streams the local file at localPath to remotePath. The local file
// is opened inside the project root, so a symlinked path component pointing out
// of the project fails the upload instead of shipping a file from outside it.
func uploadFile(conn remote.Client, root *fs.Root, localPath, remotePath string) error {
	src, err := root.Open(localPath)
	if err != nil {
		return fmt.Errorf("open local %s: %w", localPath, err)
	}
	uploadErr := conn.Upload(remotePath, src)
	closeErr := src.Close()
	if err := errors.Join(uploadErr, closeErr); err != nil {
		return fmt.Errorf("upload %s to %s: %w", localPath, remotePath, err)
	}
	return nil
}

// downloadFile writes the remote file at remotePath to localPath inside the
// project root. WriteAtomic closes the remote stream, so a transfer that only
// fails on close never reaches the target file.
func downloadFile(conn remote.Client, root *fs.Root, remotePath, localPath string) error {
	src, err := conn.Open(remotePath)
	if err != nil {
		return fmt.Errorf("open remote %s: %w", remotePath, err)
	}
	if err := root.WriteAtomic(localPath, src); err != nil {
		return fmt.Errorf("download %s to %s: %w", remotePath, localPath, err)
	}
	return nil
}
