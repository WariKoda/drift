package browser

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/charmbracelet/x/ansi"
)

// testEntries builds n local entries named file0…file(n-1).
func testEntries(n int) []*fs.FileEntry {
	entries := make([]*fs.FileEntry, n)
	for i := range entries {
		entries[i] = &fs.FileEntry{
			Name: "file" + string(rune('0'+i%10)),
			Path: "/project/file" + string(rune('0'+i%10)),
			Kind: fs.EntryFile,
		}
	}
	return entries
}

// testModel builds a browser sized 80x24: panes 39 and 40 wide with the
// divider at x=39, entry rows at y=4…21.
func testModel(localCount, remoteCount int) Model {
	m := Model{
		WorkDir:         "/project",
		entries:         testEntries(localCount),
		Selection:       fs.NewSelectionState(),
		RemoteSelection: fs.NewSelectionState(),
		Width:           80,
		Height:          24,
	}
	if remoteCount > 0 {
		m.remoteEntries = testEntries(remoteCount)
	}
	return m
}

func TestHitTest(t *testing.T) {
	tests := []struct {
		name         string
		localCount   int
		remoteCount  int
		offset       int
		remoteOffset int
		x, y         int
		wantZone     zone
		wantIndex    int
	}{
		{
			name: "first entry in the local pane",
			localCount: 10, x: 5, y: 4,
			wantZone: zoneLocal, wantIndex: 0,
		},
		{
			name: "last visible entry in the local pane",
			localCount: 30, x: 5, y: 21,
			wantZone: zoneLocal, wantIndex: 17,
		},
		{
			name: "a scrolled local pane adds its offset",
			localCount: 30, offset: 7, x: 5, y: 4,
			wantZone: zoneLocal, wantIndex: 7,
		},
		{
			name: "the divider column belongs to no pane",
			localCount: 10, remoteCount: 10, x: 39, y: 4,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "first entry in the remote pane",
			localCount: 10, remoteCount: 10, x: 50, y: 4,
			wantZone: zoneRemote, wantIndex: 0,
		},
		{
			name: "a scrolled remote pane adds its own offset",
			localCount: 10, remoteCount: 30, remoteOffset: 4, x: 50, y: 6,
			wantZone: zoneRemote, wantIndex: 6,
		},
		{
			name: "the remote pane is empty without a host",
			localCount: 10, x: 50, y: 4,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "blank filler below the last entry",
			localCount: 2, x: 5, y: 10,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the header row is not selectable",
			localCount: 10, x: 5, y: 0,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the separator above the entries is not selectable",
			localCount: 10, x: 5, y: 3,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the separator below the entries is not selectable",
			localCount: 30, x: 5, y: 22,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the status bar is not selectable",
			localCount: 30, x: 5, y: 23,
			wantZone: zoneNone, wantIndex: -1,
		},
		{
			name: "the local pane label activates the local pane",
			localCount: 10, x: 5, y: paneLabelRow,
			wantZone: zoneLocalLabel, wantIndex: -1,
		},
		{
			name: "the remote pane label activates the remote pane",
			localCount: 10, remoteCount: 10, x: 50, y: paneLabelRow,
			wantZone: zoneRemoteLabel, wantIndex: -1,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			m := testModel(tc.localCount, tc.remoteCount)
			m.offset = tc.offset
			m.remoteOffset = tc.remoteOffset

			got := m.hitTest(tc.x, tc.y)
			if got.zone != tc.wantZone || got.index != tc.wantIndex {
				t.Errorf("hitTest(%d,%d) = {zone %v, index %d}, want {zone %v, index %d}",
					tc.x, tc.y, got.zone, got.index, tc.wantZone, tc.wantIndex)
			}
		})
	}
}

func TestHitTestPreviewPane(t *testing.T) {
	// preview.source is the pane the previewed file came from; the preview
	// itself renders on the opposite side.
	m := testModel(10, 10)
	m.preview.active = true
	m.preview.source = PaneLocal

	if got := m.hitTest(50, 4); got.zone != zonePreview {
		t.Errorf("right pane shows the preview, got zone %v", got.zone)
	}
	if got := m.hitTest(5, 4); got.zone != zoneLocal || got.index != 0 {
		t.Errorf("left pane still lists local entries, got {%v, %d}", got.zone, got.index)
	}

	m.preview.source = PaneRemote
	if got := m.hitTest(5, 4); got.zone != zonePreview {
		t.Errorf("left pane shows the preview, got zone %v", got.zone)
	}
	if got := m.hitTest(50, 4); got.zone != zoneRemote || got.index != 0 {
		t.Errorf("right pane still lists remote entries, got {%v, %d}", got.zone, got.index)
	}
}

func TestHitTestFinder(t *testing.T) {
	m := testModel(10, 0)
	m.finder.active = true
	m.finder.results = make([]finderResult, 20)

	tests := []struct {
		name      string
		offset    int
		loading   bool
		noResults bool
		y         int
		wantZone  zone
		wantIndex int
	}{
		{name: "first result row", y: finderResultsTop, wantZone: zoneFinder, wantIndex: 0},
		{name: "scrolled results add the offset", offset: 5, y: finderResultsTop, wantZone: zoneFinder, wantIndex: 5},
		{name: "the query prompt is not a result", y: 1, wantZone: zoneNone, wantIndex: -1},
		{name: "the help line is not a result", y: 23, wantZone: zoneNone, wantIndex: -1},
		{name: "the indexing message is not a result", loading: true, y: finderResultsTop, wantZone: zoneNone, wantIndex: -1},
		{name: "the no-matches message is not a result", noResults: true, y: finderResultsTop, wantZone: zoneNone, wantIndex: -1},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			f := m
			f.finder.offset = tc.offset
			f.finder.loading = tc.loading
			if tc.noResults {
				f.finder.results = nil
			}

			// x must not matter: the overlay spans the full width.
			for _, x := range []int{0, 39, 70} {
				got := f.hitTest(x, tc.y)
				if got.zone != tc.wantZone || got.index != tc.wantIndex {
					t.Errorf("hitTest(%d,%d) = {%v, %d}, want {%v, %d}",
						x, tc.y, got.zone, got.index, tc.wantZone, tc.wantIndex)
				}
			}
		})
	}
}

func TestHitTestHelpOverlaySelectsNothing(t *testing.T) {
	m := testModel(10, 0)
	m.showHelp = true

	if got := m.hitTest(5, 4); got.zone != zoneNone {
		t.Errorf("the help overlay has nothing selectable, got zone %v", got.zone)
	}
}

// TestLayoutConstantsMatchView is the test that makes a layout change fail
// loudly. It renders the real view and checks that the rows the hit test
// computes from the constants really do hold what the constants claim.
func TestLayoutConstantsMatchView(t *testing.T) {
	m := testModel(3, 0)
	m.entries[0].Name = "alpha-marker"
	m.entries[1].Name = "beta-marker"

	lines := strings.Split(m.View(), "\n")
	if len(lines) < m.Height {
		t.Fatalf("view rendered %d lines, want at least %d", len(lines), m.Height)
	}

	t.Run("entriesTop is the first entry row", func(t *testing.T) {
		row := ansi.Strip(lines[entriesTop])
		if !strings.Contains(row, "alpha-marker") {
			t.Errorf("row %d = %q, want the first entry", entriesTop, row)
		}
	})

	t.Run("the row after entriesTop is the second entry", func(t *testing.T) {
		row := ansi.Strip(lines[entriesTop+1])
		if !strings.Contains(row, "beta-marker") {
			t.Errorf("row %d = %q, want the second entry", entriesTop+1, row)
		}
	})

	t.Run("paneLabelRow holds the pane labels", func(t *testing.T) {
		row := ansi.Strip(lines[paneLabelRow])
		if !strings.Contains(row, "LOCAL") {
			t.Errorf("row %d = %q, want the pane labels", paneLabelRow, row)
		}
	})

	t.Run("the viewport ends before the footer", func(t *testing.T) {
		// The last entry row plus the footer must fill the terminal exactly.
		lastEntryRow := entriesTop + m.viewportHeight() - 1
		if lastEntryRow+footerLines != m.Height-1 {
			t.Errorf("last entry row %d + %d footer lines = %d, want %d",
				lastEntryRow, footerLines, lastEntryRow+footerLines, m.Height-1)
		}
	})
}

// TestFinderLayoutConstantsMatchView does the same for the finder overlay.
func TestFinderLayoutConstantsMatchView(t *testing.T) {
	m := testModel(0, 0)
	m.finder.active = true
	m.finder.results = []finderResult{
		{rel: "alpha-marker.go", abs: "/project/alpha-marker.go"},
		{rel: "beta-marker.go", abs: "/project/beta-marker.go"},
	}

	lines := strings.Split(m.View(), "\n")
	if len(lines) < m.Height {
		t.Fatalf("view rendered %d lines, want at least %d", len(lines), m.Height)
	}

	row := ansi.Strip(lines[finderResultsTop])
	if !strings.Contains(row, "alpha-marker.go") {
		t.Errorf("row %d = %q, want the first finder result", finderResultsTop, row)
	}

	lastResultRow := finderResultsTop + m.finderViewportHeight() - 1
	if lastResultRow+finderFooterLines != m.Height-1 {
		t.Errorf("last result row %d + %d footer lines = %d, want %d",
			lastResultRow, finderFooterLines, lastResultRow+finderFooterLines, m.Height-1)
	}
}
