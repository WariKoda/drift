package diffview

import (
	"strings"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/styles"
)

// streamKind classifies a row of the continuous all-files view.
type streamKind uint8

const (
	streamTitle streamKind = iota // file title; a click collapses the section
	streamLine                    // display row of the file's diff
	streamNote                    // summary or error text instead of a text diff
	streamGap                     // blank row closing a section
)

type streamRow struct {
	session int
	kind    streamKind
	index   int // display row for streamLine, note line for streamNote
}

// streamPlan lays out every file below its title. It is rebuilt when folds,
// collapsed sections, the layout, a preview direction or an error changes.
type streamPlan struct {
	rows     []streamRow
	start    []int               // title row per session
	files    [][]diff.DisplayRow // display rows per session
	revision uint64
	split    bool
	dirs     []SyncDir
	failed   []bool
}

// streamAnchor identifies the top row independently of row indices, so a
// rebuilt plan can keep the reading position.
type streamAnchor struct {
	session int
	kind    streamKind
	source  int // DiffLine index for streamLine, note line for streamNote
}

func (m Model) continuous() bool { return m.preferences.Continuous }

func (m Model) syncDir(index int) SyncDir {
	if index < 0 || index >= len(m.syncDirs) {
		return DirNone
	}
	return m.syncDirs[index]
}

func (m Model) streamCurrent(plan *streamPlan) bool {
	if plan == nil || plan.revision != m.foldRevision || plan.split != m.splitLayout() || len(plan.dirs) != len(m.sessions) {
		return false
	}
	for i := range m.sessions {
		if plan.dirs[i] != m.syncDir(i) || plan.failed[i] != (m.sessions[i].Err != nil) {
			return false
		}
	}
	return true
}

// hasTextRows reports whether a session shows display rows rather than notes,
// matching the single-file view.
func (m Model) hasTextRows(index int) bool {
	session := &m.sessions[index]
	return session.Err == nil && session.Result != nil && !session.Result.Binary && len(session.Result.Lines) > 0 && !m.longHidden(index)
}

func (m Model) buildStream() *streamPlan {
	plan := &streamPlan{
		start:    make([]int, len(m.sessions)),
		files:    make([][]diff.DisplayRow, len(m.sessions)),
		revision: m.foldRevision,
		split:    m.splitLayout(),
		dirs:     make([]SyncDir, len(m.sessions)),
		failed:   make([]bool, len(m.sessions)),
	}
	for i := range m.sessions {
		session := &m.sessions[i]
		plan.dirs[i], plan.failed[i] = m.syncDir(i), session.Err != nil
		plan.start[i] = len(plan.rows)
		plan.rows = append(plan.rows, streamRow{session: i, kind: streamTitle})
		if !m.collapsed[i] {
			if m.hasTextRows(i) {
				plan.files[i] = m.rowsFor(i)
				for index := range plan.files[i] {
					plan.rows = append(plan.rows, streamRow{session: i, kind: streamLine, index: index})
				}
			} else {
				for index := range m.noteLines(i) {
					plan.rows = append(plan.rows, streamRow{session: i, kind: streamNote, index: index})
				}
			}
		}
		plan.rows = append(plan.rows, streamRow{session: i, kind: streamGap})
	}
	return plan
}

// stream returns the prepared plan, or builds one for a model that has not
// been prepared yet (value receivers in tests and View).
func (m Model) streamPlan() *streamPlan {
	if m.streamCurrent(m.stream) {
		return m.stream
	}
	return m.buildStream()
}

func (p *streamPlan) anchor(row int) (streamAnchor, bool) {
	if row < 0 || row >= len(p.rows) {
		return streamAnchor{}, false
	}
	r := p.rows[row]
	anchor := streamAnchor{session: r.session, kind: r.kind, source: r.index}
	if r.kind == streamLine {
		anchor.source = p.files[r.session][r.index].SourceLine()
	}
	return anchor, true
}

func (p *streamPlan) indexOf(anchor streamAnchor) int {
	if anchor.session < 0 || anchor.session >= len(p.start) {
		return 0
	}
	start := p.start[anchor.session]
	end := len(p.rows) - 1
	if anchor.session+1 < len(p.start) {
		end = p.start[anchor.session+1] - 1
	}
	switch anchor.kind {
	case streamLine:
		if rows := p.files[anchor.session]; len(rows) > 0 {
			return start + 1 + diff.IndexOfSourceLine(rows, anchor.source)
		}
	case streamNote:
		return min(start+1+anchor.source, end)
	case streamGap:
		return end
	}
	return start
}

// prepareStream rebuilds a stale plan and keeps the top row's content in view.
func (m *Model) prepareStream() {
	if !m.continuous() {
		m.stream = nil
		return
	}
	if m.streamCurrent(m.stream) {
		return
	}
	var anchor streamAnchor
	ok := false
	if m.stream != nil {
		anchor, ok = m.stream.anchor(m.scroll)
	}
	m.stream = m.buildStream()
	m.hunk = nil
	if ok {
		m.scroll = m.stream.indexOf(anchor)
	}
	m.clampScroll()
}

// setContinuous switches between one file and all files at the same position.
func (m *Model) setContinuous(on bool) {
	if on == m.continuous() {
		return
	}
	if on {
		source, deep := m.topSourceLine(), m.scroll > 0
		m.preferences.Continuous = true
		m.stream = nil
		m.prepareDisplayRows()
		if len(m.sessions) > 0 {
			m.scroll = m.stream.start[m.activeIdx]
			if deep && m.hasTextRows(m.activeIdx) {
				m.scroll = m.stream.indexOf(streamAnchor{session: m.activeIdx, kind: streamLine, source: source})
			}
		}
	} else {
		anchor, ok := m.stream.anchor(m.scroll)
		m.preferences.Continuous = false
		m.stream = nil
		m.prepareDisplayRows()
		if ok && anchor.session == m.activeIdx && anchor.kind == streamLine {
			m.remapScrollToSource(anchor.source)
		} else {
			m.scrollToFirstDifference()
		}
	}
	m.hunk = nil
	m.clampScroll()
	m.clampHorizontal()
}

// visibleSessions lists the files with at least one row on screen, top first.
func (m Model) visibleSessions() []int {
	plan := m.streamPlan()
	var sessions []int
	for row := max(0, m.scroll); row < len(plan.rows) && row < m.scroll+m.rowsOnScreen(); row++ {
		if session := plan.rows[row].session; len(sessions) == 0 || sessions[len(sessions)-1] != session {
			sessions = append(sessions, session)
		}
	}
	return sessions
}

// setActive changes the file that sync keys act on without moving the diff.
func (m *Model) setActive(index int) {
	if index < 0 || index >= len(m.sessions) || index == m.activeIdx {
		return
	}
	m.activeIdx = index
	m.clampFileList()
}

// followScroll makes the file at the top of the viewport active after manual
// scrolling. The blank row closing a section already belongs to the next file.
func (m *Model) followScroll() {
	plan := m.streamPlan()
	if m.scroll < 0 || m.scroll >= len(plan.rows) {
		return
	}
	row := plan.rows[m.scroll]
	index := row.session
	if row.kind == streamGap && index+1 < len(m.sessions) {
		index++
	}
	m.setActive(index)
}

// selectSection puts a file's title at the top and makes it active.
func (m *Model) selectSection(index int) {
	if index < 0 || index >= len(m.sessions) {
		return
	}
	m.prepareDisplayRows()
	m.hunk = nil
	m.scroll = m.stream.start[index]
	m.clampScroll()
	m.activeIdx = index
	m.clampFileList()
}

func (m *Model) toggleSection(index int) {
	if index < 0 || index >= len(m.sessions) {
		return
	}
	if m.collapsed == nil {
		m.collapsed = make(map[int]bool)
	}
	m.collapsed[index] = !m.collapsed[index]
	m.foldRevision++
	m.setActive(index)
	m.prepareDisplayRows()
}

func (m *Model) jumpStreamHunk(direction int) {
	m.prepareDisplayRows()
	plan := m.stream
	from := m.scroll
	if m.hunk != nil && m.hunk.stream == plan && m.hunk.scroll == m.scroll {
		from = m.hunk.row
	}
	for row := from + direction; row >= 0 && row < len(plan.rows); row += direction {
		r := plan.rows[row]
		if r.kind == streamLine && plan.files[r.session][r.index].Kind == diff.DisplayHunkHeader {
			m.scroll = row
			m.clampScroll()
			m.hunk = &hunkPosition{stream: plan, row: row, scroll: m.scroll}
			m.setActive(r.session)
			return
		}
	}
}

// visibleSpans returns, per file on screen, the first and last visible
// display rows of its diff.
func (m Model) visibleSpans() map[int][2]int {
	plan := m.streamPlan()
	spans := make(map[int][2]int)
	for row := max(0, m.scroll); row < len(plan.rows) && row < m.scroll+m.rowsOnScreen(); row++ {
		r := plan.rows[row]
		if r.kind != streamLine {
			continue
		}
		span, ok := spans[r.session]
		if !ok {
			span[0] = r.index
		}
		span[1] = r.index
		spans[r.session] = span
	}
	return spans
}

func (m Model) firstVisibleStreamFold() (int, diff.DisplayRow, bool) {
	plan := m.streamPlan()
	for row := max(0, m.scroll); row < len(plan.rows) && row < m.scroll+m.rowsOnScreen(); row++ {
		r := plan.rows[row]
		if r.kind == streamLine && plan.files[r.session][r.index].Kind == diff.DisplayFold {
			return r.session, plan.files[r.session][r.index], true
		}
	}
	return 0, diff.DisplayRow{}, false
}

func (m *Model) toggleFirstVisibleStreamFold() {
	if session, row, ok := m.firstVisibleStreamFold(); ok {
		m.setGapExpandedIn(session, row.GapID, !m.gapExpandedIn(session, row.GapID))
		m.prepareDisplayRows()
	}
}

func (m *Model) collapseVisibleStreamFold() {
	plan := m.streamPlan()
	spans := m.visibleSpans()
	for _, session := range m.visibleSessions() {
		span, ok := spans[session]
		if ok && m.collapseFoldIn(session, plan.files[session], span[0], span[1]) {
			m.prepareDisplayRows()
			return
		}
	}
}

// noteLines replaces the text diff of a file without one.
func (m Model) noteLines(index int) []string {
	session := &m.sessions[index]
	switch {
	case session.Err != nil:
		return []string{"  " + styles.Err.Render(styles.SafeText(session.Err.Error()))}
	case session.Result == nil:
		return []string{"  " + styles.Muted.Render("loading…")}
	case m.longHidden(index):
		return m.longNote(index)
	default:
		return summaryLines(session.Result)
	}
}

func (m Model) renderStreamRows(height, width int) []string {
	plan := m.streamPlan()
	rows := make([]string, 0, height)
	for row := max(0, m.scroll); row < len(plan.rows) && len(rows) < height; {
		r := plan.rows[row]
		switch r.kind {
		case streamTitle:
			rows = append(rows, m.renderSectionTitle(r.session, width))
		case streamNote:
			rows = append(rows, pad(m.noteLines(r.session)[r.index], width))
		case streamGap:
			rows = append(rows, strings.Repeat(" ", width))
		case streamLine:
			end := row
			for end < len(plan.rows) && plan.rows[end].kind == streamLine && plan.rows[end].session == r.session && end-row < height-len(rows) {
				end++
			}
			session := &m.sessions[r.session]
			segment := plan.files[r.session][r.index : r.index+end-row]
			rows = append(rows, diff.RenderRows(session.Result, segment, width, height-len(rows), m.renderOptions(r.session))...)
			row = end
			continue
		}
		row++
	}
	for len(rows) < height {
		rows = append(rows, strings.Repeat(" ", width))
	}
	return rows
}

// renderSectionTitle marks the active file and whether its section is open.
func (m Model) renderSectionTitle(index, width int) string {
	marker := "▾ "
	if m.collapsed[index] {
		marker = "▸ "
	}
	style := styles.Muted
	if index == m.activeIdx {
		style = styles.Key
	}
	if width < 3 {
		return pad(style.Render(marker), width)
	}
	return style.Render(marker) + m.renderFileTitle(index, width-2)
}
