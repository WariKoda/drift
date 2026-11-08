package diffview

import (
	"crypto/sha256"
	"encoding/binary"

	"github.com/WariKoda/drift/internal/diff"
)

type fileKey struct{ local, remote string }
type lineAnchor struct {
	line    diff.DiffLine
	context [32]byte
	header  bool
}
type fileViewState struct {
	anchor lineAnchor
	gaps   [][32]byte
}

// ViewSnapshot contains only reading state, never transfers or sync decisions.
// The root binds it to one refresh request and discards it on scope replacement.
type ViewSnapshot struct {
	active fileKey
	files  map[fileKey]fileViewState
}

func (m Model) topSourceLine() int {
	rows := m.displayRows()
	if m.scroll >= 0 && m.scroll < len(rows) {
		return rows[m.scroll].SourceLine()
	}
	return 0
}

func (m *Model) selectFile(index int) {
	if index < 0 || index >= len(m.sessions) || index == m.activeIdx {
		return
	}
	if m.savedScroll == nil {
		m.savedScroll = make(map[int]int)
	}
	m.savedScroll[m.activeIdx] = m.scroll
	m.activeIdx = index
	m.foldRevision++
	m.prepareDisplayRows()
	m.clampFileList()
	if scroll, ok := m.savedScroll[index]; ok {
		m.scroll = scroll
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
	snapshot := ViewSnapshot{files: make(map[fileKey]fileViewState)}
	for i, session := range m.sessions {
		if session.Result == nil {
			continue
		}
		scroll, visited := m.savedScroll[i]
		if i == m.activeIdx {
			scroll, visited = m.scroll, true
		}
		if !visited {
			continue
		}
		flip := i < len(m.syncDirs) && (m.syncDirs[i] == DirUpload || m.syncDirs[i] == DirDeleteRemote)
		rows := diff.Flatten(session.Result.Lines, diff.DefaultContext, m.expandedFor(i), flip)
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
		state := fileViewState{anchor: lineAnchor{line: session.Result.Lines[index], context: lineContext(session.Result.Lines, index), header: row.Kind == diff.DisplayHunkHeader}}
		for gap := range m.expandedFor(i) {
			state.gaps = append(state.gaps, gapFingerprint(session.Result.Lines, gap))
		}
		key := fileKey{session.LocalPath, session.RemotePath}
		snapshot.files[key] = state
		if i == m.activeIdx {
			snapshot.active = key
		}
	}
	return snapshot
}

func (m *Model) RestoreView(snapshot ViewSnapshot) {
	m.savedScroll = make(map[int]int)
	for i, session := range m.sessions {
		key := fileKey{session.LocalPath, session.RemotePath}
		state, ok := snapshot.files[key]
		if !ok || session.Result == nil {
			continue
		}
		lines := session.Result.Lines
		if len(state.gaps) > 0 {
			matches := make(map[[32]byte][]int)
			for _, gap := range diff.FoldableGapIDs(lines, diff.DefaultContext) {
				fingerprint := gapFingerprint(lines, gap)
				matches[fingerprint] = append(matches[fingerprint], gap)
			}
			for _, fingerprint := range state.gaps {
				if ids := matches[fingerprint]; len(ids) == 1 {
					m.ensureExpanded(i)[ids[0]] = struct{}{}
				}
			}
		}
		index := -1
		for j, line := range lines {
			if line.Kind == state.anchor.line.Kind && line.Text == state.anchor.line.Text && lineContext(lines, j) == state.anchor.context {
				index = j
				break
			}
		}
		if index >= 0 {
			flip := i < len(m.syncDirs) && (m.syncDirs[i] == DirUpload || m.syncDirs[i] == DirDeleteRemote)
			rows := diff.Flatten(lines, diff.DefaultContext, m.expandedFor(i), flip)
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
		}
		if key == snapshot.active {
			m.activeIdx = i
		}
	}
	m.foldRevision++
	m.prepareDisplayRows()
	m.clampFileList()
	if scroll, ok := m.savedScroll[m.activeIdx]; ok {
		m.scroll = scroll
		m.clampScroll()
	} else {
		m.scrollToFirstDifference()
	}
}
