// Package progress shares the progress and cancellation of one long-running
// operation between the goroutine doing the work and whoever displays it.
package progress

import (
	"context"
	"errors"
	"sync"
)

// Progress describes the current phase of a network operation.
type Progress struct {
	Phase         string
	Done          int
	Total         int
	Indeterminate bool
}

// Tracker safely shares progress between a tea.Cmd goroutine and the TUI.
type Tracker struct {
	mu       sync.Mutex
	progress Progress
	done     bool
	ctx      context.Context
	cancel   context.CancelFunc
}

// NewTracker creates an indeterminate tracker with the given initial phase.
func NewTracker(phase string) *Tracker {
	ctx, cancel := context.WithCancel(context.Background())
	t := &Tracker{ctx: ctx, cancel: cancel}
	t.Set(phase, 0, 0, true)
	return t
}

// Context is canceled when the user aborts the operation.
func (t *Tracker) Context() context.Context {
	if t == nil {
		return context.Background()
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.ctx == nil {
		return context.Background()
	}
	return t.ctx
}

// Cancel asks the running operation to stop. Already-finished work is kept.
func (t *Tracker) Cancel() {
	if t == nil {
		return
	}
	t.mu.Lock()
	cancel := t.cancel
	t.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// Canceled reports whether Cancel has been called.
func (t *Tracker) Canceled() bool {
	if t == nil {
		return false
	}
	return t.Context().Err() != nil
}

// IsCanceled reports whether err is a user abort rather than a real failure.
func IsCanceled(err error) bool {
	return err != nil && errors.Is(err, context.Canceled)
}

// Set replaces the current progress values.
func (t *Tracker) Set(phase string, done, total int, indeterminate bool) {
	if t == nil {
		return
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	t.progress = Progress{Phase: phase, Done: done, Total: total, Indeterminate: indeterminate}
}

// Inc advances the completed counter by one.
func (t *Tracker) Inc() {
	if t == nil {
		return
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	t.progress.Done++
}

// Finish marks the tracked operation as complete.
func (t *Tracker) Finish() {
	if t == nil {
		return
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	t.done = true
}

// Snapshot returns a consistent progress snapshot.
func (t *Tracker) Snapshot() (Progress, bool) {
	if t == nil {
		return Progress{}, false
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.progress, t.done
}
