package dashboard

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/project"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func dispatchMsg(m Model, msg tea.Msg) (Model, tea.Msg) {
	m, cmd := m.Update(msg)
	if cmd == nil {
		return m, nil
	}
	return m, cmd()
}

func leftPress(x, y int) tea.MouseMsg {
	return tea.MouseMsg{
		X:      x,
		Y:      y,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	}
}

func wheel(button tea.MouseButton) tea.MouseMsg {
	return tea.MouseMsg{Button: button}
}

func TestDashboardHitTest(t *testing.T) {
	m := newModel(t) // 80x24, two projects (Alpha, Zeta)
	top := m.listTop()
	left := m.blockLeft()
	right := left + m.blockWidth() - 1

	if top != 8 {
		t.Fatalf("listTop() = %d, want 8 (1 blank + 5 logo + 2 blanks)", top)
	}
	if m.blockWidth() != 58 {
		t.Fatalf("blockWidth at 80 cols = %d, want 58", m.blockWidth())
	}
	if left != 11 {
		t.Fatalf("blockLeft at 80 cols = %d, want 11", left)
	}

	// Layout of an 80x24 dashboard with 2 projects:
	//
	//   y 0       blank padding
	//   y 1–5     logo
	//   y 6–7     blank padding
	//   y 8       first project  (listTop)
	//   y 9       second project
	//   y 10–19   blank filler (listMax is 12; only 2 rows used)
	//   y 20–23   footer
	tests := []struct {
		name string
		x, y int
		want int
	}{
		{"the blank padding above the logo is not selectable", 40, 0, noHit},
		{"the logo is not selectable", 40, 3, noHit},
		{"the blank padding below the logo is not selectable", 40, 7, noHit},
		{"the first project, left edge of the block", left, top, 0},
		{"the first project, inside the block", left + 10, top, 0},
		{"the first project, right edge of the block", right, top, 0},
		{"the second project", left + 10, top + 1, 1},
		{"a click left of the centered block", left - 1, top, noHit},
		{"a click right of the centered block", right + 1, top, noHit},
		{"x=0 is outside the block", 0, top, noHit},
		{"x=79 is outside the block", 79, top, noHit},
		{"a blank row below the last entry", left + 10, top + 2, noHit},
		{"the action bar is not selectable", left + 10, 20, noHit},
		{"the status bar is not selectable", left + 10, 22, noHit},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := m.hitTest(tc.x, tc.y); got != tc.want {
				t.Errorf("hitTest(%d,%d) = %d, want %d", tc.x, tc.y, got, tc.want)
			}
		})
	}
}

func TestDashboardHitTestWindowedList(t *testing.T) {
	dir := t.TempDir()
	projects := make([]project.Project, 20)
	for i := range projects {
		projects[i] = project.Project{
			Slug: string(rune('a' + i)),
			Name: "P" + string(rune('A'+i)),
			Path: dir,
		}
	}
	m := New(&project.Registry{Projects: projects}, 80, 24)
	m.cursor = 15
	m.clampCursor()

	start := m.windowStart()
	if start == 0 {
		t.Fatal("expected the window to have scrolled")
	}
	x := m.blockLeft() + 5
	if got := m.hitTest(x, m.listTop()); got != start {
		t.Errorf("top visible row hitTest = %d, want windowStart %d", got, start)
	}
}

func TestDashboardLayoutMatchesView(t *testing.T) {
	m := newModel(t)
	lines := strings.Split(m.View(), "\n")
	if len(lines) != m.Height {
		t.Fatalf("view rendered %d lines, want %d", len(lines), m.Height)
	}

	t.Run("listTop is the first project row", func(t *testing.T) {
		row := ansi.Strip(lines[m.listTop()])
		if !strings.Contains(row, "Alpha") {
			t.Errorf("row %d = %q, want the first project", m.listTop(), row)
		}
	})

	t.Run("the row after listTop is the second project", func(t *testing.T) {
		row := ansi.Strip(lines[m.listTop()+1])
		if !strings.Contains(row, "Zeta") {
			t.Errorf("row %d = %q, want the second project", m.listTop()+1, row)
		}
	})

	t.Run("the first project row is centered at blockLeft", func(t *testing.T) {
		row := ansi.Strip(lines[m.listTop()])
		pad := len(row) - len(strings.TrimLeft(row, " "))
		if pad != m.blockLeft() {
			t.Errorf("first project row has %d leading spaces, blockLeft()=%d", pad, m.blockLeft())
		}
	})

	t.Run("logoRowCount matches the rendered logo", func(t *testing.T) {
		if got := len(logoLines()); got != logoRowCount {
			t.Errorf("logoLines has %d rows, logoRowCount=%d", got, logoRowCount)
		}
	})
}

func TestBlockWidthScalesWithTerminal(t *testing.T) {
	at80 := Model{Width: 80, Height: 24}
	if at80.blockWidth() != 58 || at80.nameWidth() != 18 {
		t.Errorf("80-col layout = %d/%d, want 58/18", at80.blockWidth(), at80.nameWidth())
	}

	wide := Model{Width: 120, Height: 24}
	if wide.blockWidth() != 88 {
		t.Errorf("120-col blockWidth = %d, want 88 (capped)", wide.blockWidth())
	}
	if wide.nameWidth() != 36 {
		t.Errorf("120-col nameWidth = %d, want 36 (capped)", wide.nameWidth())
	}
	if wide.blockWidth() <= at80.blockWidth() {
		t.Error("wide terminal should grow the block")
	}

	narrow := Model{Width: 50, Height: 24}
	if narrow.blockWidth() > 48 {
		t.Errorf("50-col blockWidth = %d, want <= 48", narrow.blockWidth())
	}
	if narrow.blockWidth() < 40 {
		t.Errorf("50-col blockWidth = %d, want >= 40", narrow.blockWidth())
	}
}

func TestWheelMovesCursor(t *testing.T) {
	m := newModel(t)
	m.statusMsg = "stale"
	if m.cursor != 0 {
		t.Fatalf("initial cursor = %d, want 0", m.cursor)
	}

	m, _ = dispatchMsg(m, wheel(tea.MouseButtonWheelDown))
	if m.cursor != 1 {
		t.Fatalf("cursor after wheel down = %d, want 1", m.cursor)
	}
	if m.statusMsg != "" {
		t.Fatalf("wheel should clear statusMsg, got %q", m.statusMsg)
	}

	m, _ = dispatchMsg(m, wheel(tea.MouseButtonWheelDown))
	if m.cursor != 1 {
		t.Fatalf("cursor after wheel down at bottom = %d, want 1", m.cursor)
	}

	m, _ = dispatchMsg(m, wheel(tea.MouseButtonWheelUp))
	if m.cursor != 0 {
		t.Fatalf("cursor after wheel up = %d, want 0", m.cursor)
	}
}

func TestClickMovesCursor(t *testing.T) {
	m := newModel(t)
	x := m.blockLeft() + 5
	y := m.listTop() + 1

	m, out := dispatchMsg(m, leftPress(x, y))
	if out != nil {
		t.Fatalf("single click should not emit, got %T", out)
	}
	if m.cursor != 1 {
		t.Fatalf("cursor after click = %d, want 1", m.cursor)
	}
}

func TestDoubleClickOpensProject(t *testing.T) {
	m := newModel(t)
	x := m.blockLeft() + 5
	y := m.listTop()

	m, out := dispatchMsg(m, leftPress(x, y))
	if out != nil {
		t.Fatalf("first click should not emit, got %T", out)
	}
	m, out = dispatchMsg(m, leftPress(x, y))
	chosen, ok := out.(MsgProjectChosen)
	if !ok {
		t.Fatalf("expected MsgProjectChosen on double click, got %T", out)
	}
	if chosen.Project.Slug != "a" {
		t.Fatalf("chosen slug = %q, want a", chosen.Project.Slug)
	}
}

func TestDoubleClickMissingPathDoesNotEmit(t *testing.T) {
	reg := &project.Registry{Projects: []project.Project{
		{Slug: "gone", Name: "Gone", Path: "/no/such/path/drift-test"},
	}}
	m := New(reg, 80, 24)
	x := m.blockLeft() + 5
	y := m.listTop()

	m, _ = dispatchMsg(m, leftPress(x, y))
	m, out := dispatchMsg(m, leftPress(x, y))
	if out != nil {
		t.Fatalf("missing path should not emit chosen, got %T", out)
	}
	if m.statusMsg == "" {
		t.Fatal("expected a status message for missing path")
	}
}

func TestMouseIgnoredDuringConfirmDelete(t *testing.T) {
	m := newModel(t)
	m.confirmDelete = true
	x := m.blockLeft() + 5

	m, out := dispatchMsg(m, leftPress(x, m.listTop()+1))
	if out != nil {
		t.Fatalf("click during confirm should not emit, got %T", out)
	}
	if m.cursor != 0 {
		t.Fatalf("cursor during confirm = %d, want 0", m.cursor)
	}

	m, _ = dispatchMsg(m, wheel(tea.MouseButtonWheelDown))
	if m.cursor != 0 {
		t.Fatalf("wheel during confirm moved cursor to %d", m.cursor)
	}
}

func TestClickOutsideBlockDoesNotMoveCursor(t *testing.T) {
	m := newModel(t)
	m.cursor = 1

	m, _ = dispatchMsg(m, leftPress(0, m.listTop()))
	if m.cursor != 1 {
		t.Fatalf("click outside block moved cursor to %d", m.cursor)
	}
}
