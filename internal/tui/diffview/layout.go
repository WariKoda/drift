package diffview

import (
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

const autoSplitColumns = 160

func (m *Model) SetPreferences(preferences config.DiffPreferences) {
	anchor := m.topSourceLine()
	if m.preferences.Syntax != preferences.Syntax {
		if m.decoration != nil {
			m.decoration.cancel()
			m.decoration = nil
		}
		m.syntax = nil
	}
	m.preferences = preferences
	m.prepareDisplayRows()
	m.remapScrollToSource(anchor)
	m.clampHorizontal()
}

func (m Model) splitLayout() bool {
	return m.preferences.Layout == "split" || (m.preferences.Layout == "auto" && m.diffWidth() >= autoSplitColumns)
}

func (m Model) rowsFor(index int) []diff.DisplayRow {
	if index < 0 || index >= len(m.sessions) || m.sessions[index].Result == nil {
		return nil
	}
	flip := index < len(m.syncDirs) && (m.syncDirs[index] == DirUpload || m.syncDirs[index] == DirDeleteRemote)
	lines := m.sessions[index].Result.Lines
	rows := diff.Flatten(lines, diff.DefaultContext, m.expandedFor(index), flip)
	if m.splitLayout() {
		rows = diff.SplitDisplayRows(lines, rows, flip)
	}
	return rows
}

func (m *Model) clampHorizontal() {
	if !m.columnsReady {
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
	m.horizontal = min(max(0, m.horizontal), max(0, m.maxColumns-max(0, width-gutter)))
}
