package diffview

import "github.com/WariKoda/drift/internal/diff"

func (m Model) wrapping() bool { return m.preferences.Wrapping() }

// rowHeight is the number of terminal rows of scrollable row index.
func (m Model) rowHeight(index int) int {
	if !m.wrapping() {
		return 1
	}
	if m.continuous() {
		plan := m.streamPlan()
		if index < 0 || index >= len(plan.rows) || plan.rows[index].kind != streamLine {
			return 1
		}
		r := plan.rows[index]
		return diff.RowHeight(m.sessions[r.session].Result, plan.files[r.session][r.index], m.diffWidth(), m.renderOptions(r.session))
	}
	rows := m.displayRows()
	s := m.activeSession()
	if index < 0 || index >= len(rows) || s == nil {
		return 1
	}
	return diff.RowHeight(s.Result, rows[index], m.diffWidth(), m.renderOptions(m.activeIdx))
}

// rowsOnScreen counts the scrollable rows that are at least partly visible.
// Without wrapping it is the viewport height, as before.
func (m Model) rowsOnScreen() int {
	if !m.wrapping() {
		return m.viewportHeight()
	}
	used, count := 0, 0
	for index := max(0, m.scroll); index < m.totalLines() && used < m.viewportHeight(); index++ {
		used += m.rowHeight(index)
		count++
	}
	return max(1, count)
}

// scrollLimit is the last scroll position that still fills the viewport. A
// final row taller than the viewport can still be scrolled to.
func (m Model) scrollLimit() int {
	total := m.totalLines()
	if !m.wrapping() {
		return max(0, total-m.viewportHeight())
	}
	used := 0
	for index := total - 1; index >= 0; index-- {
		used += m.rowHeight(index)
		if used > m.viewportHeight() {
			return min(index+1, total-1)
		}
	}
	return 0
}

// rowAt maps a viewport line to the scrollable row drawn there.
func (m Model) rowAt(line int) int {
	if !m.wrapping() {
		return m.scroll + line
	}
	used := 0
	for index := max(0, m.scroll); index < m.totalLines(); index++ {
		used += m.rowHeight(index)
		if line < used {
			return index
		}
	}
	return max(m.scroll, m.totalLines()) + line - used // past the last row, as without wrapping
}

func (m *Model) toggleWrap() {
	wrap := !m.wrapping()
	m.preferences.Wrap = &wrap
	m.horizontal = 0
	m.clampScroll()
}
