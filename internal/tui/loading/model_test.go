package loading

import (
	"strings"
	"testing"
	"time"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

func TestIndicatorUsesConfiguredDelay(t *testing.T) {
	if showDelay != 200*time.Millisecond {
		t.Fatalf("show delay = %s, want 200ms", showDelay)
	}
}

func TestIndicatorIsDelayedAndCanBeHidden(t *testing.T) {
	tracker := NewTracker("Connecting…")
	var model Model
	if cmd := model.Start("Connecting…", tracker); cmd == nil {
		t.Fatal("start did not schedule the delayed indicator")
	}
	if !model.Active() {
		t.Fatal("started indicator is not active")
	}
	if model.Visible() || model.BackgroundVisible() {
		t.Fatal("indicator was visible before its delay elapsed")
	}

	tracker.Set("Comparing files…", 2, 5, false)
	if cmd := model.Update(showMsg{id: model.id}); cmd == nil {
		t.Fatal("show message did not start animation ticks")
	}
	if !model.Visible() {
		t.Fatal("indicator was not visible after its delayed show message")
	}

	model.Hide()
	if model.Visible() || !model.BackgroundVisible() {
		t.Fatal("hidden indicator did not switch to background status")
	}
	if got := model.Status(); got != "Comparing files… 2/5" {
		t.Fatalf("status = %q, want tracked progress", got)
	}
}

func TestFinishedIndicatorIgnoresStaleShow(t *testing.T) {
	var model Model
	model.Start("Connecting…", nil)
	staleID := model.id
	model.Finish()

	if cmd := model.Update(showMsg{id: staleID}); cmd != nil {
		t.Fatal("stale show message scheduled an animation tick")
	}
	if model.Active() || model.Visible() || model.BackgroundVisible() {
		t.Fatal("stale show message revived a finished indicator")
	}
}

func TestOverlayKeepsTerminalDimensionsAndBackground(t *testing.T) {
	var model Model
	model.Start("Comparing files…", nil)
	model.Update(showMsg{id: model.id})

	base := strings.Repeat("visible background\n", 11) + "visible background"
	view := model.Overlay(base, 60, 12)
	lines := strings.Split(view, "\n")
	if len(lines) != 12 {
		t.Fatalf("overlay has %d lines, want 12", len(lines))
	}
	for i, line := range lines {
		if width := lipgloss.Width(line); width != 60 {
			t.Fatalf("line %d has width %d, want 60", i, width)
		}
	}
	plain := ansi.Strip(view)
	if !strings.Contains(plain, "visible background") {
		t.Fatal("overlay removed the underlying view")
	}
	if !strings.Contains(plain, "Comparing files…") || !strings.Contains(plain, "[Esc] hide") || !strings.Contains(plain, "[q] cancel") {
		t.Fatal("overlay does not contain its activity content")
	}
}

func TestCancelStopsTrackerAndClearsIndicator(t *testing.T) {
	tracker := NewTracker("Connecting…")
	var model Model
	model.Start("Connecting…", tracker)
	model.Update(showMsg{id: model.id})

	model.Cancel()
	if !tracker.Canceled() {
		t.Fatal("cancel did not abort the tracker")
	}
	if model.Active() || model.Visible() || model.BackgroundVisible() {
		t.Fatal("cancelled indicator remained visible")
	}
}

func TestHideDoesNotCancelTracker(t *testing.T) {
	tracker := NewTracker("Connecting…")
	var model Model
	model.Start("Connecting…", tracker)
	model.Update(showMsg{id: model.id})
	model.Hide()
	if tracker.Canceled() {
		t.Fatal("hide aborted the tracker")
	}
	if !model.BackgroundVisible() {
		t.Fatal("hidden indicator did not stay active in the background")
	}
}
