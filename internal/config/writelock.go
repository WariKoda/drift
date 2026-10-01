package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"golang.org/x/sys/unix"
)

// ErrWriteBusy is returned rather than blocking the UI while another process
// performs a management operation. The lock file is permanent: never unlink it.
var ErrWriteBusy = errors.New("configuration is being changed by another drift process; retry")

// WriteLock coordinates a complete management transaction with the Rust app.
// It owns the same flock inode at <config.Dir()>/write.lock in both programs.
type WriteLock struct {
	file *os.File
	dir  string
}

func LockWrites() (*WriteLock, error) {
	dir := Dir()
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, err
	}
	file, err := os.OpenFile(filepath.Join(dir, "write.lock"), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, err
	}
	if err := unix.Flock(int(file.Fd()), unix.LOCK_EX|unix.LOCK_NB); err != nil {
		closeErr := file.Close()
		if errors.Is(err, unix.EWOULDBLOCK) {
			err = ErrWriteBusy
		}
		return nil, errors.Join(err, closeErr)
	}
	return &WriteLock{file: file, dir: dir}, nil
}

func (lock *WriteLock) Valid() bool { return lock != nil && lock.file != nil && lock.dir == Dir() }
func (lock *WriteLock) Close() error {
	if lock == nil || lock.file == nil {
		return nil
	}
	file := lock.file
	lock.file = nil
	return file.Close() // closing releases flock, including after an error
}

// WithWriteLock runs read, validation and all writes under one process lock.
func WithWriteLock(fn func(*WriteLock) error) (result error) {
	lock, err := LockWrites()
	if err != nil {
		return fmt.Errorf("lock configuration: %w", err)
	}
	defer func() { result = errors.Join(result, lock.Close()) }()
	return fn(lock)
}

// ConflictError keeps an open form from overwriting an externally changed record.
type ConflictError struct{ Record string }

func (err *ConflictError) Error() string {
	return err.Record + " changed in another drift process; reload before saving"
}
