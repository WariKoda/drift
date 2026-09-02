package diffview

import (
	"errors"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
)

// testDiffModel builds a diff view sized 80x24 with n loaded sessions.
// With n >= 5 the file list is 5 rows tall: rows y=2…6, content from y=10.
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
	tests := []struct {
		name           string
		sessions       int
		fileListOffset int
		scroll         int
		y              int
		wantZone       zone
		wantIndex      int
	}{
		{
			name: "first file row", sessions: 5, y: fileListTop,
			wantZone: zoneFileList, wantIndex: 0,
		},
		{
			name: "last file row", sessions: 5, y: fileListTop + 4,
			wantZone: zoneFileList, wantIndex: 4,
		},
		{
			name: "a scrolled file list adds its offset", sessions: 10, fileListOffset: 3, y: fileListTop,
			wantZone: zoneFileList, wantIndex: 3,
		},
		{
			name: "the header row is not selectable", sessions: 5, y: 0,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the separator below the file list is not selectable", sessions: 5, y: fileListTop + 5,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the status bar is not selectable", sessions: 5, y: 23,
			wantZone: zoneNone, wantIndex: -1,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			m := testDiffModel(tc.sessions)
			m.fileListOffset = tc.fileListOffset
			m.scroll = tc.scroll

			// x must not matter: both bands span the full width.
			for _, x := range []int{0, 39, 70} {
				got := m.hitTest(x, tc.y)
				if got.zone != tc.wantZone || got.index != tc.wantIndex {
					t.Errorf("hitTest(%d,%d) = {%v, %d}, want {%v, %d}",
						x, tc.y, got.zone, got.index, tc.wantZone, tc.wantIndex)
				}
			}
		})
	}
}

func TestDiffHitTestContentBand(t *testing.T) {
	m := testDiffModel(5)
	top := m.contentTop()

	t.Run("first content row maps to the scroll position", func(t *testing.T) {
		m.scroll = 12
		got := m.hitTest(5, top)
		if got.zone != zoneContent || got.index != 12 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, 12}", got.zone, got.index)
		}
	})

	t.Run("the error overlay borrows the rows but has no diff lines", func(t *testing.T) {
		e := testDiffModel(5)
		e.showErrors = true
		got := e.hitTest(5, top)
		if got.zone != zoneContent || got.index != -1 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, -1}", got.zone, got.index)
		}
	})

	t.Run("a failed session has no diff lines", func(t *testing.T) {
		e := testDiffModel(5)
		e.sessions[0].Err = errors.New("boom")
		got := e.hitTest(5, top)
		if got.zone != zoneContent || got.index != -1 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, -1}", got.zone, got.index)
		}
	})

	t.Run("a still-loading session has no diff lines", func(t *testing.T) {
		e := testDiffModel(5)
		e.sessions[0].Result = nil
		got := e.hitTest(5, top)
		if got.zone != zoneContent || got.index != -1 {
			t.Errorf("hitTest = {%v, %d}, want {zoneContent, -1}", got.zone, got.index)
		}
	})
}

func TestDiffHitTestBlankFileRow(t *testing.T) {
	// Two sessions make the list two rows tall, so there is no blank row to
	// hit inside it; a third row already belongs to the separator below.
	m := testDiffModel(2)
	if got := m.hitTest(5, fileListTop+2); got.zone == zoneFileList {
		t.Errorf("row below the last session should not be a file row, got %v", got)
	}
}

// TestDiffLayoutConstantsMatchView binds the constants to the rendered view, so
// a layout change fails here instead of silently shifting every click.
func TestDiffLayoutConstantsMatchView(t *testing.T) {
	m := testDiffModel(5)

	if m.fileListHeight() != 5 {
		t.Fatalf("fileListHeight = %d, want 5", m.fileListHeight())
	}

	t.Run("the bands fill the terminal exactly", func(t *testing.T) {
		total := headerLines + m.fileListHeight() + midLines + m.viewportHeight() + footerLines
		if total != m.Height {
			t.Errorf("layout sums to %d lines, want %d", total, m.Height)
		}
	})

	t.Run("contentTop follows the file list and its labels", func(t *testing.T) {
		want := fileListTop + m.fileListHeight() + midLines
		if m.contentTop() != want {
			t.Errorf("contentTop = %d, want %d", m.contentTop(), want)
		}
	})

	t.Run("chromeLines matches the individual bands", func(t *testing.T) {
		if chromeLines != headerLines+midLines+footerLines {
			t.Errorf("chromeLines = %d, want %d", chromeLines, headerLines+midLines+footerLines)
		}
	})
}

func TestClampFileListOffsetDoesNotFollowCursor(t *testing.T) {
	m := testDiffModel(20)
	m.activeIdx = 0
	m.fileListOffset = 9

	m.clampFileListOffset()

	if m.fileListOffset != 9 {
		t.Errorf("offset = %d, want 9 — the wheel must not be dragged back to the cursor", m.fileListOffset)
	}

	m.fileListOffset = 99
	m.clampFileListOffset()
	if want := 20 - m.fileListHeight(); m.fileListOffset != want {
		t.Errorf("offset = %d, want %d", m.fileListOffset, want)
	}

	m.fileListOffset = -5
	m.clampFileListOffset()
	if m.fileListOffset != 0 {
		t.Errorf("offset = %d, want 0", m.fileListOffset)
	}
}
