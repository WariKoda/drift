package diffview

import (
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

const autoSplitColumns = 160

func (m *Model) SetPreferences(preferences config.DiffPreferences) {
	anchor := m.topSourceLine()
	if m.preferences.Syntax != preferences.Syntax {
		m.resetDecorations()
	}
	continuous := preferences.Continuous
	preferences.Continuous = m.preferences.Continuous
	m.preferences = preferences
	m.prepareDisplayRows()
	m.remapScrollToSource(anchor)
	m.setContinuous(continuous)
	m.clampHorizontal()
}

func (m Model) splitLayout() bool {
	return m.preferences.Layout == "split" || (m.preferences.Layout == "auto" && m.diffWidth() >= autoSplitColumns)
}

func (m Model) rowsFor(index int) []diff.DisplayRow {
	if index < 0 || index >= len(m.sessions) || m.sessions[index].Result == nil || m.longHidden(index) {
		return nil
	}
	flip := m.sessionFlip(index)
	lines := m.sessions[index].Result.Lines
	rows := diff.FlattenWith(lines, m.context(), m.expandedFor(index), m.revealed[index], flip)
	diff.LabelHunks(lines, rows, flip, sessionName(&m.sessions[index]))
	if m.splitLayout() {
		rows = diff.SplitDisplayRows(lines, rows, flip)
	}
	return rows
}

func (m *Model) clampHorizontal() {
	columns, ready := m.columnLimit()
	if !ready {
		m.horizontal = max(0, m.horizontal)
		return
	}
	width := m.diffWidth()
	if m.splitLayout() {
		width = (width - 1) / 2
	}
	gutter := 12
	if s := m.activeSession(); s != nil && s.Result != nil {
		gutter = 2*s.Result.LineNumberWidth() + 4
		if m.splitLayout() {
			gutter = s.Result.LineNumberWidth() + 3
		}
	}
	m.horizontal = min(max(0, m.horizontal), max(0, columns-max(0, width-gutter)))
}
