// Package log provides drift's opt-in file logging. It is a thin wrapper around
// log/slog that is a no-op until Init is called, so logging stays off by default
// and costs nothing when disabled. All helpers are safe for concurrent use,
// which matters because drift logs from tea.Cmd goroutines.
package log

import (
	"io"
	"log/slog"
	"os"
	"path/filepath"
)

// Options configures Init. Path is the log file; Debug raises the level from
// Info to Debug.
type Options struct {
	Path  string
	Debug bool
}

var (
	logger  *slog.Logger
	enabled bool
)

// Init opens opts.Path for appending and routes all subsequent log calls there.
// It returns the file as an io.Closer so the caller can close it on exit. Until
// Init succeeds the package helpers are no-ops.
func Init(opts Options) (io.Closer, error) {
	if dir := filepath.Dir(opts.Path); dir != "" {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, err
		}
	}
	f, err := os.OpenFile(opts.Path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return nil, err
	}
	level := slog.LevelInfo
	if opts.Debug {
		level = slog.LevelDebug
	}
	logger = slog.New(slog.NewTextHandler(f, &slog.HandlerOptions{Level: level}))
	enabled = true
	return f, nil
}

// Enabled reports whether logging is active.
func Enabled() bool { return enabled }

// Info logs at info level. No-op when logging is disabled.
func Info(msg string, args ...any) {
	if enabled {
		logger.Info(msg, args...)
	}
}

// Error logs at error level. No-op when logging is disabled.
func Error(msg string, args ...any) {
	if enabled {
		logger.Error(msg, args...)
	}
}

// Debug logs at debug level. No-op when logging is disabled.
func Debug(msg string, args ...any) {
	if enabled {
		logger.Debug(msg, args...)
	}
}
