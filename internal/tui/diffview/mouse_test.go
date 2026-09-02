package diffview

import (
	"errors"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
)

// testDiffModel builds a diff view sized 80x24 with n loaded sessions.
// Body fills y=2…21 (20 rows); file list is left of x=fileListWidth.
func testDiffModel(n int) Model {
	sessions := make([]diff.Session, n)
	for i := range sessions {
		sessions[i] = diff.Session{
			LocalPath:  "/project/file.txt",
			RemotePath: "/remote/file.txt",
			Loaded:     true,
			Result:     &diff.DiffResult{},
		}
	}
	return Model{
		sessions: sessions,
		syncDirs: make([]SyncDir, n),
		Width:    80,
		Height:   24,
	}
}

func TestDiffHitTest(t *testing.T) {
	m := testDiffModel(5)
	fw := m.fileListWidth()

	tests := []struct {
		name           string
		sessions       int
		fileListOffset int
		x, y           int
		wantZone       zone
		wantIndex      int
	}{
		{
			name: "first file row", sessions: 5, x: 0, y: bodyTop,
			wantZone: zoneFileList, wantIndex: 0,
		},
		{
			name: "file row near bottom of body", sessions: 20, x: 1, y: bodyTop + 4,
			wantZone: zoneFileList, wantIndex: 4,
		},
		{
			name: "a scrolled file list adds its offset", sessions: 30, fileListOffset: 3, x: 0, y: bodyTop,
			wantZone: zoneFileList, wantIndex: 3,
		},
		{
			name: "the header row is not selectable", sessions: 5, x: 0, y: 0,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the vertical divider is not selectable", sessions: 5, x: fw, y: bodyTop,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "right-pane path header is not selectable", sessions: 5, x: fw + 1, y: bodyTop,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the status bar is not selectable", sessions: 5, x: 0, y: 23,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "click on the right of a file row is the content pane, not the file", sessions: 5, x: fw + 1, y: bodyTop + pathChrome,
			wantZone: zoneContent, wantIndex: 0,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			model := testDiffModel(tc.sessions)
			model.fileListOffset = tc.fileListOffset

			got := model.hitTest(tc.x, tc.y)
			if got.zone != tc.wantZone || got.index != tc.wantIndex {
				t.Errorf("hitTest(%d,%d) = {%v, %d}, want {%v, %d}",
					tc.x, tc.y, got.zone, got.index, tc.wantZone, tc.wantIndex)
			}
		})
	}
}

func TestDiffHitTestContentBand(t *testing.T) {
	m := testDiffModel(5)
	top := m.contentTop()
	x := m.fileListWidth() + 1

	t.Run("first content row maps to the scroll position", func(t *testing.T) {
		m.scroll = 12
		got := m.hitTest(x, top)
		if got.zone != zoneContent || got.index != 12 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, 12}", got.zone, got.index)
		}
	})

	t.Run("the error overlay borrows the rows but has no diff lines", func(t *testing.T) {
		e := testDiffModel(5)
		e.showErrors = true
		got := e.hitTest(x, top)
		if got.zone != zoneContent || got.index != -1 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, -1}", got.zone, got.index)
		}
	})

	t.Run("a failed session has no diff lines", func(t *testing.T) {
		e := testDiffModel(5)
		e.sessions[0].Err = errors.New("boom")
		got := e.hitTest(x, top)
		if got.zone != zoneContent || got.index != -1 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, -1}", got.zone, got.index)
		}
	})

	t.Run("a still-loading session has no diff lines", func(t *testing.T) {
		e := testDiffModel(5)
		e.sessions[0].Result = nil
		got := e.hitTest(x, top)
		if got.zone != zoneContent || got.index != -1 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, -1}", got.zone, got.index)
		}
	})

	t.Run("left of the divider stays the file list even on content rows", func(t *testing.T) {
		got := m.hitTest(0, top)
		if got.zone != zoneFileList || got.index != pathChrome {
			t.Errorf("hitTest = {%v, %d}, want {zoneFileList, %d}", got.zone, got.index, pathChrome)
		}
	})
}

func TestDiffHitTestBlankFileRow(t *testing.T) {
	m := testDiffModel(2)
	// Body is taller than two sessions; rows past the last session are blank.
	if got := m.hitTest(0, bodyTop+2); got.zone == zoneFileList {
		t.Errorf("row below the last session should not be a file row, got %v", got)
	}
}

// TestDiffLayoutConstantsMatchView binds the constants to the rendered view, so
// a layout change fails here instead of silently shifting every click.
func TestDiffLayoutConstantsMatchView(t *testing.T) {
	m := testDiffModel(5)

	t.Run("the bands fill the terminal exactly", func(t *testing.T) {
		total := headerLines + m.bodyHeight() + footerLines
		if total != m.Height {
			t.Errorf("layout sums to %d lines, want %d", total, m.Height)
		}
	})

	t.Run("file list and diff share the body height", func(t *testing.T) {
		if m.fileListHeight() != m.bodyHeight() {
			t.Errorf("fileListHeight = %d, want bodyHeight %d", m.fileListHeight(), m.bodyHeight())
		}
		if m.fileListHeight() != pathChrome+m.viewportHeight() {
			t.Errorf("fileListHeight = %d, want pathChrome+viewport %d",
				m.fileListHeight(), pathChrome+m.viewportHeight())
		}
	})

	t.Run("contentTop follows the path chrome", func(t *testing.T) {
		want := bodyTop + pathChrome
		if m.contentTop() != want {
			t.Errorf("contentTop = %d, want %d", m.contentTop(), want)
		}
	})

	t.Run("widths add up with the divider", func(t *testing.T) {
		if m.fileListWidth()+1+m.diffWidth() != m.Width {
			t.Errorf("widths %d+1+%d = %d, want %d",
				m.fileListWidth(), m.diffWidth(), m.fileListWidth()+1+m.diffWidth(), m.Width)
		}
	})
}

func TestClampFileListOffsetDoesNotFollowCursor(t *testing.T) {
	m := testDiffModel(40)
	m.activeIdx = 0
	m.fileListOffset = 9

	m.clampFileListOffset()

	if m.fileListOffset != 9 {
		t.Errorf("offset = %d, want 9 — the wheel must not be dragged back to the cursor", m.fileListOffset)
	}

	m.fileListOffset = 99
	m.clampFileListOffset()
	if want := 40 - m.fileListHeight(); m.fileListOffset != want {
		t.Errorf("offset = %d, want %d", m.fileListOffset, want)
	}

	m.fileListOffset = -5
	m.clampFileListOffset()
	if m.fileListOffset != 0 {
		t.Errorf("offset = %d, want 0", m.fileListOffset)
	}
}
