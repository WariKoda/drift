// Package loading provides the global network activity indicator used by TUI screens.
package loading

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

const showDelay = 200 * time.Millisecond
const tickInterval = 100 * time.Millisecond

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

// Model owns the visual state of one global network operation.
type Model struct {
	active   bool
	visible  bool
	revealed bool
	label    string
	progress Progress
	tracker  *Tracker
	frame    int
	id       uint64
}

type showMsg struct{ id uint64 }
type tickMsg struct{ id uint64 }

// Start begins a new activity and schedules its delayed display.
func (m *Model) Start(label string, tracker *Tracker) tea.Cmd {
	if tracker == nil {
		tracker = NewTracker(label)
	}
	m.id++
	m.active = true
	m.visible = false
	m.revealed = false
	m.label = label
	m.tracker = tracker
	m.frame = 0
	m.progress = Progress{Phase: label, Indeterminate: true}
	m.progress, _ = tracker.Snapshot()
	id := m.id
	return tea.Tick(showDelay, func(time.Time) tea.Msg { return showMsg{id: id} })
}

// Finish clears the current activity. Delayed messages for it become stale.
func (m *Model) Finish() {
	m.id++
	m.active = false
	m.visible = false
	m.revealed = false
	m.label = ""
	m.progress = Progress{}
	m.tracker = nil
	m.frame = 0
}

// Hide dismisses the modal without stopping the underlying operation.
func (m *Model) Hide() {
	m.visible = false
}

// Cancel aborts the running operation and clears the indicator immediately.
func (m *Model) Cancel() {
	if m.tracker != nil {
		m.tracker.Cancel()
	}
	m.Finish()
}

// Tracker returns the tracker for the current activity, if any.
func (m Model) Tracker() *Tracker { return m.tracker }

// Update advances delayed display, spinner animation, and tracked progress.
func (m *Model) Update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case showMsg:
		if !m.active || msg.id != m.id {
			return nil
		}
		m.visible = true
		m.revealed = true
		m.snapshot()
		return tickCmd(msg.id)
	case tickMsg:
		if !m.active || msg.id != m.id {
			return nil
		}
		m.frame++
		m.snapshot()
		return tickCmd(msg.id)
	}
	return nil
}

func (m *Model) snapshot() {
	if m.tracker == nil {
		return
	}
	progress, _ := m.tracker.Snapshot()
	m.progress = progress
}

func tickCmd(id uint64) tea.Cmd {
	return tea.Tick(tickInterval, func(time.Time) tea.Msg { return tickMsg{id: id} })
}

// Active reports whether a network operation is still running.
func (m Model) Active() bool { return m.active }

// Visible reports whether the full-screen modal is currently shown.
func (m Model) Visible() bool { return m.active && m.visible }

// BackgroundVisible reports whether a revealed modal was dismissed with Esc.
func (m Model) BackgroundVisible() bool {
	return m.active && m.revealed && !m.visible
}

// Status returns a compact status-line description for a hidden operation.
func (m Model) Status() string {
	phase := m.progress.Phase
	if phase == "" {
		phase = m.label
	}
	if m.progress.Total > 0 && !m.progress.Indeterminate {
		done := m.progress.Done
		if done > m.progress.Total {
			done = m.progress.Total
		}
		return fmt.Sprintf("%s %d/%d", phase, done, m.progress.Total)
	}
	return phase
}
