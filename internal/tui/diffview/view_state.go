package diffview

import (
	"crypto/sha256"
	"encoding/binary"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

type fileKey struct{ local, remote string }
type lineAnchor struct {
	line    diff.DiffLine
	context [32]byte
	header  bool
}
type fileViewState struct {
	anchor     lineAnchor
	gaps       [][32]byte
	reveals    map[[32]byte]int
	horizontal int
}

// ViewSnapshot contains only reading state, never transfers or sync decisions.
// The root binds it to one refresh request and discards it on scope replacement.
type ViewSnapshot struct {
	active       fileKey
	files        map[fileKey]fileViewState
	preferences  config.DiffPreferences
	contextShift int
	opened       map[fileKey]bool // long-line files shown with [o]
}

func (m Model) topSourceLine() int {
	rows := m.displayRows()
	if m.scroll >= 0 && m.scroll < len(rows) {
		return rows[m.scroll].SourceLine()
	}
	return 0
}

func (m *Model) selectFile(index int) {
	if m.continuous() {
		m.selectSection(index)
		return
	}
	if index < 0 || index >= len(m.sessions) || index == m.activeIdx {
		return
	}
	if m.savedScroll == nil {
		m.savedScroll = make(map[int]int)
	}
	m.savedScroll[m.activeIdx] = m.scroll
	if m.savedSource == nil {
		m.savedSource = make(map[int]int)
	}
	if m.savedHorizontal == nil {
		m.savedHorizontal = make(map[int]int)
	}
	m.savedSource[m.activeIdx] = m.topSourceLine()
	m.savedHorizontal[m.activeIdx] = m.horizontal
	m.horizontal = m.savedHorizontal[index]
	m.activeIdx = index
	m.foldRevision++
	m.prepareDisplayRows()
	m.clampFileList()
	if scroll, ok := m.savedScroll[index]; ok {
		m.scroll = scroll
		if source, ok := m.savedSource[index]; ok {
			m.scroll = diff.IndexOfSourceLine(m.displayRows(), source)
		}
		m.clampScroll()
	} else {
		m.scrollToFirstDifference()
	}
}

func lineContext(lines []diff.DiffLine, index int) [32]byte {
	hash := sha256.New()
	for i := max(0, index-2); i < min(len(lines), index+3); i++ {
		_, _ = hash.Write([]byte{byte(lines[i].Kind)})
		var size [8]byte
		binary.LittleEndian.PutUint64(size[:], uint64(len(lines[i].Text)))
		_, _ = hash.Write(size[:])
		_, _ = hash.Write([]byte(lines[i].Text))
	}
	return [32]byte(hash.Sum(nil))
}

func gapFingerprint(lines []diff.DiffLine, start int) [32]byte {
	hash := sha256.New()
	for i := start; i < len(lines) && lines[i].Kind == diff.LineEqual; i++ {
		var size [8]byte
		binary.LittleEndian.PutUint64(size[:], uint64(len(lines[i].Text)))
		_, _ = hash.Write(size[:])
		_, _ = hash.Write([]byte(lines[i].Text))
	}
	return [32]byte(hash.Sum(nil))
}

func (m Model) SnapshotView() ViewSnapshot {
	snapshot := ViewSnapshot{files: make(map[fileKey]fileViewState), preferences: m.preferences, contextShift: m.contextShift, opened: make(map[fileKey]bool)}
	for i, open := range m.longOpen {
		if open && i < len(m.sessions) {
			snapshot.opened[fileKey{m.sessions[i].LocalPath, m.sessions[i].RemotePath}] = true
		}
	}
	for i, session := range m.sessions {
		if session.Result == nil {
			continue
		}
		scroll, visited := m.savedScroll[i]
		if i == m.activeIdx {
			scroll, visited = m.fileScroll(), true
		}
		if !visited {
			continue
		}
		rows := m.rowsFor(i)
		if source, ok := m.savedSource[i]; ok && i != m.activeIdx {
			scroll = diff.IndexOfSourceLine(rows, source)
		}
		if i == m.activeIdx {
			rows = m.displayRows()
		}
		if len(rows) == 0 {
			continue
		}
		row := rows[min(max(0, scroll), len(rows)-1)]
		index := row.SourceLine()
		if index < 0 || index >= len(session.Result.Lines) {
			continue
		}
		horizontal := m.savedHorizontal[i]
		if i == m.activeIdx {
			horizontal = m.horizontal
		}
		state := fileViewState{anchor: lineAnchor{line: session.Result.Lines[index], context: lineContext(session.Result.Lines, index), header: row.Kind == diff.DisplayHunkHeader}, horizontal: horizontal}
		for gap := range m.expandedFor(i) {
			state.gaps = append(state.gaps, gapFingerprint(session.Result.Lines, gap))
		}
		for gap, lines := range m.revealed[i] {
			if state.reveals == nil {
				state.reveals = make(map[[32]byte]int)
			}
			state.reveals[gapFingerprint(session.Result.Lines, gap)] = lines
		}
		key := fileKey{session.LocalPath, session.RemotePath}
		snapshot.files[key] = state
		if i == m.activeIdx {
			snapshot.active = key
		}
	}
	return snapshot
}

// fileScroll is the active file's display row at the top of the viewport. In
// continuous mode a title or note at the top counts as the file's first row.
func (m Model) fileScroll() int {
	if !m.continuous() {
		return m.scroll
	}
	plan := m.streamPlan()
	if m.scroll >= 0 && m.scroll < len(plan.rows) {
		if row := plan.rows[m.scroll]; row.session == m.activeIdx && row.kind == streamLine {
			return row.index
		}
	}
	return 0
}

func (m *Model) RestoreView(snapshot ViewSnapshot) {
	m.preferences = snapshot.preferences
	m.contextShift = snapshot.contextShift
	m.stream = nil
	for i, session := range m.sessions {
		if snapshot.opened[fileKey{session.LocalPath, session.RemotePath}] {
			if m.longOpen == nil {
				m.longOpen = make(map[int]bool)
			}
			m.longOpen[i] = true
		}
	}
	m.savedScroll = make(map[int]int)
	m.savedSource = make(map[int]int)
	m.savedHorizontal = make(map[int]int)
	for i, session := range m.sessions {
		key := fileKey{session.LocalPath, session.RemotePath}
		state, ok := snapshot.files[key]
		if !ok || session.Result == nil {
			continue
		}
		lines := session.Result.Lines
		if len(state.gaps) > 0 || len(state.reveals) > 0 {
			matches := make(map[[32]byte][]int)
			for _, gap := range diff.FoldableGapIDs(lines, m.context()) {
				fingerprint := gapFingerprint(lines, gap)
				matches[fingerprint] = append(matches[fingerprint], gap)
			}
			for _, fingerprint := range state.gaps {
				if ids := matches[fingerprint]; len(ids) == 1 {
					m.ensureExpanded(i)[ids[0]] = struct{}{}
				}
			}
			for fingerprint, extra := range state.reveals {
				if ids := matches[fingerprint]; len(ids) == 1 {
					if m.revealed == nil {
						m.revealed = make(map[int]map[int]int)
					}
					if m.revealed[i] == nil {
						m.revealed[i] = make(map[int]int)
					}
					m.revealed[i][ids[0]] = extra
				}
			}
		}
		index := -1
		for j, line := range lines {
			if line.Kind == state.anchor.line.Kind && line.Text == state.anchor.line.Text && lineContext(lines, j) == state.anchor.context {
				if index >= 0 {
					index = -1
					break
				}
				index = j
			}
		}
		if index >= 0 {
			rows := m.rowsFor(i)
			scroll := diff.IndexOfSourceLine(rows, index)
			if state.anchor.header {
				for j, row := range rows {
					if row.Kind == diff.DisplayHunkHeader && row.LineIndex == index {
						scroll = j
						break
					}
				}
			}
			m.savedScroll[i] = scroll
			m.savedSource[i] = index
			m.savedHorizontal[i] = state.horizontal
		}
		if key == snapshot.active {
			m.activeIdx = i
		}
	}
	m.foldRevision++
	m.prepareDisplayRows()
	m.clampFileList()
	m.horizontal = m.savedHorizontal[m.activeIdx]
	m.clampHorizontal()
	if m.continuous() {
		if m.activeIdx < len(m.sessions) {
			m.scroll = m.stream.start[m.activeIdx]
			if source, ok := m.savedSource[m.activeIdx]; ok {
				m.scroll = m.stream.indexOf(streamAnchor{session: m.activeIdx, kind: streamLine, source: source})
			}
		}
		m.clampScroll()
		return
	}
	if scroll, ok := m.savedScroll[m.activeIdx]; ok {
		m.scroll = scroll
		m.clampScroll()
	} else {
		m.scrollToFirstDifference()
	}
}
