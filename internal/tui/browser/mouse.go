package browser

import (
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// zone names the region of the browser a click landed in.
type zone int

const (
	zoneNone zone = iota
	zoneLocal
	zoneRemote
	zonePreview     // the pane currently showing a file preview
	zoneLocalLabel  // the "LOCAL" pane label
	zoneRemoteLabel // the "REMOTE" pane label
	zoneFinder      // a result row in the finder overlay
)

// hit is the result of a hit test: which region, and which index within it.
// index is -1 when the region has no item at that cell.
type hit struct {
	zone  zone
	index int
}

var noHit = hit{zoneNone, -1}

// hitTest maps a terminal cell to a region of the browser.
//
// The overlays are checked in the same order View renders them, because each
// one replaces the layout entirely rather than drawing on top of it.
func (m Model) hitTest(x, y int) hit {
	if m.finder.active {
		return m.hitTestFinder(y)
	}
	if m.showHelp {
		return noHit // the help overlay has nothing to select
	}

	leftW, _ := m.paneWidths()

	// The divider column belongs to neither pane.
	if x == leftW {
		return noHit
	}
	side := PaneLocal
	if x > leftW {
		side = PaneRemote
	}

	// A preview replaces the list on the side opposite its source.
	if m.preview.active && m.preview.source != side {
		if y == paneLabelRow || (y >= entriesTop && y < entriesTop+m.viewportHeight()) {
			return hit{zonePreview, -1}
		}
		return noHit
	}

	if y == paneLabelRow {
		if side == PaneLocal {
			return hit{zoneLocalLabel, -1}
		}
		return hit{zoneRemoteLabel, -1}
	}

	if y < entriesTop || y >= entriesTop+m.viewportHeight() {
		return noHit // header, separators, status bar
	}

	row := y - entriesTop
	if side == PaneLocal {
		idx := row + m.offset
		if idx < 0 || idx >= len(m.filteredEntries()) {
			return noHit // blank filler below the last entry
		}
		return hit{zoneLocal, idx}
	}

	idx := row + m.remoteOffset
	if idx < 0 || idx >= len(m.remoteEntries) {
		return noHit
	}
	return hit{zoneRemote, idx}
}

// hitTestFinder maps a row of the finder overlay to a result index.
// The overlay spans the full width, so x plays no part.
func (m Model) hitTestFinder(y int) hit {
	if y < finderResultsTop || y >= finderResultsTop+m.finderViewportHeight() {
		return noHit
	}
	// While indexing, and when nothing matches, the result area holds a single
	// message line rather than rows.
	if m.finder.loading || len(m.finder.results) == 0 {
		return noHit
	}
	idx := y - finderResultsTop + m.finder.offset
	if idx < 0 || idx >= len(m.finder.results) {
		return noHit
	}
	return hit{zoneFinder, idx}
}

// updateMouse handles wheel and click events for the browser.
func (m Model) updateMouse(msg tea.MouseMsg) (Model, tea.Cmd) {
	if m.showHelp {
		// The help overlay covers everything; a click dismisses it.
		if mouse.IsLeftPress(msg) {
			m.showHelp = false
		}
		return m, nil
	}

	h := m.hitTest(msg.X, msg.Y)

	if delta := mouse.WheelDelta(msg); delta != 0 {
		return m.wheel(h, delta)
	}
	if !mouse.IsLeftPress(msg) {
		return m, nil
	}
	return m.click(h, msg.X, msg.Y)
}

// wheel scrolls whichever viewport sits under the pointer.
func (m Model) wheel(h hit, delta int) (Model, tea.Cmd) {
	switch h.zone {
	case zoneFinder:
		m.finder.offset += delta
		m.finder.clampOffset(m.finderViewportHeight())

	case zonePreview:
		m.preview.offset += delta
		m.clampPreviewScroll()

	case zoneLocal:
		m.offset += delta
		m.clampLocalOffset()

	case zoneRemote:
		m.remoteOffset += delta
		m.clampRemoteOffset()
	}
	return m, nil
}

// click moves the cursor, and on a double click performs the row's action.
func (m Model) click(h hit, x, y int) (Model, tea.Cmd) {
	switch h.zone {
	case zoneLocalLabel:
		mouseCmd := m.disablePreview()
		m.activePane = PaneLocal
		return m, mouseCmd

	case zoneRemoteLabel:
		var mouseCmd tea.Cmd
		if m.remoteHost != nil {
			mouseCmd = m.disablePreview()
			m.activePane = PaneRemote
		}
		return m, mouseCmd

	case zoneFinder:
		m.finder.cursor = h.index
		m.finder.clamp(m.finderViewportHeight())
		// A double click marks the file, matching Space — Enter merely closes
		// the finder, which is not what clicking a row should mean.
		if m.clicks.Register(x, y) {
			if r := m.finder.current(); r != nil {
				m.Selection.Toggle(r.abs)
			}
		}
		return m, nil

	case zoneLocal:
		var mouseCmd tea.Cmd
		if m.activePane != PaneLocal {
			mouseCmd = m.disablePreview()
			m.activePane = PaneLocal
		}
		m.cursor = h.index
		m.clampScroll()
		if m.clicks.Register(x, y) {
			var cmd tea.Cmd
			m, cmd = m.activateLocal()
			return m, tea.Batch(mouseCmd, cmd)
		}
		return m, tea.Batch(mouseCmd, m.schedulePreview())

	case zoneRemote:
		if m.remoteHost == nil {
			return m, nil
		}
		var mouseCmd tea.Cmd
		if m.activePane != PaneRemote {
			mouseCmd = m.disablePreview()
			m.activePane = PaneRemote
		}
		m.remoteCursor = h.index
		m.clampRemoteScroll()
		if m.clicks.Register(x, y) && !m.remoteBusy() {
			var cmd tea.Cmd
			m, cmd = m.updateRemoteOpen()
			return m, tea.Batch(mouseCmd, cmd)
		}
		return m, tea.Batch(mouseCmd, m.schedulePreview())
	}

	return m, nil
}

// activateLocal toggles a directory under the cursor, or opens a file, as a
// double click is expected to behave in a file browser.
func (m Model) activateLocal() (Model, tea.Cmd) {
	if m.cursor < 0 || m.cursor >= len(m.entries) {
		return m, nil
	}
	entry := m.entries[m.cursor]
	if entry.Kind != fs.EntryDir {
		return m, m.schedulePreview()
	}
	if entry.Expanded {
		m.collapseAt(m.cursor)
		m.clampScroll()
	} else if err := m.expandAt(m.cursor); err != nil {
		m.statusMsg = "Error: " + err.Error()
	} else {
		m.clampScroll()
	}
	return m, m.schedulePreview()
}
