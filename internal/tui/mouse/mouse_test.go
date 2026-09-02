package mouse

import (
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

func TestWheelDelta(t *testing.T) {
	tests := []struct {
		name   string
		button tea.MouseButton
		want   int
	}{
		{"wheel up scrolls back", tea.MouseButtonWheelUp, -ScrollStep},
		{"wheel down scrolls forward", tea.MouseButtonWheelDown, ScrollStep},
		{"horizontal wheel is ignored", tea.MouseButtonWheelLeft, 0},
		{"left button is not a wheel", tea.MouseButtonLeft, 0},
		{"no button is not a wheel", tea.MouseButtonNone, 0},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := WheelDelta(tea.MouseMsg{Button: tc.button})
			if got != tc.want {
				t.Errorf("WheelDelta(%v) = %d, want %d", tc.button, got, tc.want)
			}
		})
	}
}

func TestIsWheel(t *testing.T) {
	if !IsWheel(tea.MouseMsg{Button: tea.MouseButtonWheelDown}) {
		t.Error("wheel down should be a wheel event")
	}
	if IsWheel(tea.MouseMsg{Button: tea.MouseButtonLeft}) {
		t.Error("left button should not be a wheel event")
	}
}

func TestIsLeftPress(t *testing.T) {
	press := tea.MouseMsg{Action: tea.MouseActionPress, Button: tea.MouseButtonLeft}
	if !IsLeftPress(press) {
		t.Error("left press should be reported")
	}

	release := tea.MouseMsg{Action: tea.MouseActionRelease, Button: tea.MouseButtonLeft}
	if IsLeftPress(release) {
		t.Error("release is not a press")
	}

	right := tea.MouseMsg{Action: tea.MouseActionPress, Button: tea.MouseButtonRight}
	if IsLeftPress(right) {
		t.Error("right press is not a left press")
	}
}

func TestClickTrackerDoubleClick(t *testing.T) {
	base := time.Unix(1000, 0)

	tests := []struct {
		name   string
		clicks []struct {
			x, y int
			at   time.Time
		}
		want []bool // expected result per click
	}{
		{
			name: "second click on the same cell in time",
			clicks: []struct {
				x, y int
				at   time.Time
			}{
				{5, 4, base},
				{5, 4, base.Add(100 * time.Millisecond)},
			},
			want: []bool{false, true},
		},
		{
			name: "second click too late",
			clicks: []struct {
				x, y int
				at   time.Time
			}{
				{5, 4, base},
				{5, 4, base.Add(DoubleClickWindow + time.Millisecond)},
			},
			want: []bool{false, false},
		},
		{
			name: "second click on a different row",
			clicks: []struct {
				x, y int
				at   time.Time
			}{
				{5, 4, base},
				{5, 5, base.Add(50 * time.Millisecond)},
			},
			want: []bool{false, false},
		},
		{
			name: "second click in a different column",
			clicks: []struct {
				x, y int
				at   time.Time
			}{
				{5, 4, base},
				{6, 4, base.Add(50 * time.Millisecond)},
			},
			want: []bool{false, false},
		},
		{
			// Without the reset, a third rapid click would fire a second
			// double click and repeat whatever action it triggers.
			name: "triple click fires only one double click",
			clicks: []struct {
				x, y int
				at   time.Time
			}{
				{5, 4, base},
				{5, 4, base.Add(50 * time.Millisecond)},
				{5, 4, base.Add(100 * time.Millisecond)},
			},
			want: []bool{false, true, false},
		},
		{
			name: "fourth click completes a new double click",
			clicks: []struct {
				x, y int
				at   time.Time
			}{
				{5, 4, base},
				{5, 4, base.Add(50 * time.Millisecond)},
				{5, 4, base.Add(100 * time.Millisecond)},
				{5, 4, base.Add(150 * time.Millisecond)},
			},
			want: []bool{false, true, false, true},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			var tracker ClickTracker
			for i, c := range tc.clicks {
				got := tracker.registerAt(c.x, c.y, c.at)
				if got != tc.want[i] {
					t.Errorf("click %d at (%d,%d) = %v, want %v", i+1, c.x, c.y, got, tc.want[i])
				}
			}
		})
	}
}
