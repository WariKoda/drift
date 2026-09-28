// Package loading provides the global network activity indicator used by TUI screens.
package loading

import (
	"fmt"
	"time"

	"github.com/WariKoda/drift/internal/progress"
	tea "github.com/charmbracelet/bubbletea"
)

const showDelay = 200 * time.Millisecond
const tickInterval = 100 * time.Millisecond

// Model owns the visual state of one global network operation.
type Model struct {
	active   bool
	visible  bool
	revealed bool
	label    string
	progress progress.Progress
	tracker  *progress.Tracker
	frame    int
	id       uint64
}

type showMsg struct{ id uint64 }
type tickMsg struct{ id uint64 }

// Start begins a new activity and schedules its delayed display.
func (m *Model) Start(label string, tracker *progress.Tracker) tea.Cmd {
	if tracker == nil {
		tracker = progress.NewTracker(label)
	}
	m.id++
	m.active = true
	m.visible = false
	m.revealed = false
	m.label = label
	m.tracker = tracker
	m.frame = 0
	m.progress = progress.Progress{Phase: label, Indeterminate: true}
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
	m.progress = progress.Progress{}
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
func (m Model) Tracker() *progress.Tracker { return m.tracker }

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
