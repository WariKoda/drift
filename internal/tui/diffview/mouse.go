package diffview

import (
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// zone names the region of the screen a click landed in.
type zone int

const (
	zoneNone zone = iota
	zoneFileList
	zoneContent
)

// hit is the result of a hit test: which band, and which index within it.
// index is the session index for zoneFileList and the diff line for
// zoneContent; it is -1 when the band has no item at that row, such as a
// directory heading in the file list.
type hit struct {
	zone  zone
	index int
}

// hitTest maps a terminal cell to a region of the side-by-side diff view.
//
// The body is split horizontally: x < fileListWidth is the file list, and
// x > fileListWidth (past the │ divider) is the right pane. Within the right
// pane only the rows below the path chrome map to diff lines.
func (m Model) hitTest(x, y int) hit {
	if x < 0 || x >= m.Width {
		return hit{zoneNone, -1}
	}
	if m.comparisonEmptyState() != comparisonNotEmpty && !m.showErrors {
		return hit{zoneNone, -1}
	}
	bh := m.bodyHeight()
	if y < bodyTop || y >= bodyTop+bh {
		return hit{zoneNone, -1}
	}

	fw := m.fileListWidth()
	if x < fw {
		row := y - bodyTop + m.fileListOffset
		if row < 0 || row >= len(m.fileList().rows) {
			return hit{zoneNone, -1} // blank row past the last session
		}
		return hit{zoneFileList, m.sessionAtRow(row)}
	}
	if fw > 0 && x == fw {
		return hit{zoneNone, -1} // vertical divider
	}

	// Right pane: path chrome is not selectable content.
	top := m.contentTop()
	if y < top || y >= top+m.viewportHeight() {
		return hit{zoneNone, -1}
	}

	s := m.activeSession()
	if m.showErrors || s == nil || s.Err != nil || s.Result == nil {
		return hit{zoneContent, -1}
	}
	return hit{zoneContent, y - top + m.scroll}
}

// updateMouse handles wheel and click events for the diff view.
func (m Model) updateMouse(msg tea.MouseMsg) (next Model, cmd tea.Cmd) {
	if m.showHelp {
		return m, nil
	}
	anchor, flip, active := m.topSourceLine(), m.remoteBeforeLocal(), m.activeIdx
	defer func() {
		next.prepareDisplayRows()
		if next.activeIdx == active && next.remoteBeforeLocal() != flip {
			next.remapScrollToSource(anchor)
		}
	}()
	h := m.hitTest(msg.X, msg.Y)

	if delta := mouse.WheelDelta(msg); delta != 0 {
		switch h.zone {
		case zoneFileList:
			m.fileListOffset += delta
			m.clampFileListOffset()
		case zoneContent:
			if m.showErrors {
				m.errorOffset += delta
				m.clampErrorScroll()
			} else {
				m.hunk = nil
				m.scroll += delta
				m.clampScroll()
			}
		}
		return m, nil
	}

	if !mouse.IsLeftPress(msg) {
		return m, nil
	}

	// The error overlay is modal: nothing behind it is clickable.
	if m.showErrors {
		return m, nil
	}

	if h.zone == zoneContent && h.index >= 0 {
		rows := m.displayRows()
		if h.index < len(rows) && rows[h.index].Kind == diff.DisplayFold {
			m.toggleGap(rows[h.index].GapID)
		}
		return m, nil
	}

	if h.zone != zoneFileList || h.index < 0 {
		return m, nil
	}

	if h.index != m.activeIdx {
		m.selectFile(h.index)
		// Count this as the first click of a possible double click, but do not
		// let it complete one: the row only just became active.
		m.clicks.Register(msg.X, msg.Y)
		return m, nil
	}

	// A double click on the already-active file cycles its sync direction,
	// matching Space — the only per-row action the diff view has.
	if m.clicks.Register(msg.X, msg.Y) && !m.remoteBusy() && m.connectionError() == nil {
		m.syncDirs[m.activeIdx] = nextDir(m.syncDirs[m.activeIdx], &m.sessions[m.activeIdx])
	}

	return m, nil
}
