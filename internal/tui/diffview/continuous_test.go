package diffview

import (
	"errors"
	"fmt"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

func continuousModel(t *testing.T, width, height int, sessions ...diff.Session) Model {
	t.Helper()
	if len(sessions) == 0 {
		sessions = []diff.Session{
			{LocalPath: "/p/a.txt", Result: stateFixture()},
			{LocalPath: "/p/b.png", Result: &diff.DiffResult{Binary: true, ContentDiff: true}},
			{LocalPath: "/p/c.txt", Result: shortNavigationResult()},
			{LocalPath: "/p/d.txt", Err: errors.New("permission denied")},
			{LocalPath: "/p/e.txt", Result: stateFixture()},
		}
	}
	m := New(sessions, config.Host{}, nil, nil, width, height)
	m.SetPreferences(config.DiffPreferences{Continuous: true})
	if !m.continuous() || m.stream == nil {
		t.Fatal("continuous preference was not applied")
	}
	return m
}

func topRow(m Model) streamRow { return m.stream.rows[m.scroll] }

func TestContinuousToggleKeepsReadingPositionAndDecisions(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}, {LocalPath: "/p/b", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 14)
	m.selectFile(1)
	m.scroll += 3
	source := m.topSourceLine()
	dirs := append([]SyncDir(nil), m.syncDirs...)
	m, _ = m.handleKey(keyMsg("m"))
	if !m.continuous() || m.activeIdx != 1 {
		t.Fatalf("continuous=%v active=%d", m.continuous(), m.activeIdx)
	}
	if row := topRow(m); row.session != 1 || row.kind != streamLine || m.stream.files[1][row.index].SourceLine() != source {
		t.Fatalf("top row %+v, want source line %d of file 1", row, source)
	}
	m, _ = m.handleKey(keyMsg("m"))
	if m.continuous() || m.activeIdx != 1 || m.topSourceLine() != source {
		t.Fatalf("single view returned to file %d line %d, want 1 line %d", m.activeIdx, m.topSourceLine(), source)
	}
	for i := range dirs {
		if m.syncDirs[i] != dirs[i] {
			t.Fatal("mode switch changed a sync decision")
		}
	}
}

func TestContinuousScrollingMakesTopFileActive(t *testing.T) {
	m := continuousModel(t, 100, 14)
	first := m.activeIdx
	for m.activeIdx == first {
		before := m.scroll
		m, _ = m.handleKey(keyMsg("j"))
		if m.scroll == before {
			t.Fatal("scrolling stopped before reaching the next file")
		}
	}
	if top := topRow(m); m.activeIdx != top.session && !(top.kind == streamGap && m.activeIdx == top.session+1) {
		t.Fatalf("active %d does not match top row %+v", m.activeIdx, top)
	}
	if title := stripANSI(m.renderFileTitle(m.activeIdx, m.diffWidth())); !strings.Contains(title, "b.png") {
		t.Fatalf("sticky title does not name the active file: %q", title)
	}
	if row := m.fileList().rowOf[m.activeIdx]; row < m.fileListOffset || row >= m.fileListOffset+m.fileListHeight() {
		t.Fatal("file list did not follow")
	}
	m, _ = m.updateMouse(tea.MouseMsg{Button: tea.MouseButtonWheelUp, X: m.fileListWidth() + 1, Y: m.contentTop()})
	m, _ = m.updateMouse(tea.MouseMsg{Button: tea.MouseButtonWheelUp, X: m.fileListWidth() + 1, Y: m.contentTop()})
	m, _ = m.updateMouse(tea.MouseMsg{Button: tea.MouseButtonWheelUp, X: m.fileListWidth() + 1, Y: m.contentTop()})
	if m.activeIdx != first {
		t.Fatalf("wheel up left file %d active, want %d", m.activeIdx, first)
	}
}

func TestContinuousSelectionSurvivesClampedScrolling(t *testing.T) {
	m := continuousModel(t, 100, 40)
	last := len(m.sessions) - 1
	for m.activeIdx != last {
		m, _ = m.handleKey(keyMsg("tab"))
	}
	if topRow(m).session == last {
		t.Fatal("fixture must clamp the last section below the top")
	}
	m, _ = m.handleKey(keyMsg("j"))
	m, _ = m.handleKey(keyMsg("G"))
	if m.activeIdx != last {
		t.Fatalf("clamped scroll moved sync target to %d", m.activeIdx)
	}
	m, _ = m.handleKey(keyMsg("shift+tab"))
	if m.activeIdx != last-1 || m.scroll != min(m.stream.start[last-1], m.totalLines()-m.viewportHeight()) {
		t.Fatalf("shift+tab: active %d scroll %d", m.activeIdx, m.scroll)
	}
	m, _ = m.updateMouse(clickAt(1, bodyTop+m.fileList().rowOf[0]-m.fileListOffset))
	if m.activeIdx != 0 || m.scroll != 0 {
		t.Fatalf("file list click: active %d scroll %d", m.activeIdx, m.scroll)
	}
}

func TestContinuousHunkNavigationVisitsEveryHunkInOrder(t *testing.T) {
	for _, layout := range []string{"unified", "split"} {
		t.Run(layout, func(t *testing.T) {
			m := continuousModel(t, 220, 40)
			m.SetPreferences(config.DiffPreferences{Layout: layout, Continuous: true})
			var want []string
			for row, r := range m.stream.rows {
				if r.kind == streamLine && m.stream.files[r.session][r.index].Kind == diff.DisplayHunkHeader {
					want = append(want, fmt.Sprint(row))
				}
			}
			m.scroll = 0
			var got []string
			for range len(want) + 2 {
				m, _ = m.handleKey(keyMsg("]"))
				if m.hunk == nil {
					break
				}
				got = append(got, fmt.Sprint(m.hunk.row))
				if r := m.stream.rows[m.hunk.row]; m.activeIdx != r.session {
					t.Fatalf("hunk in file %d but file %d is active", r.session, m.activeIdx)
				}
			}
			got = got[:min(len(got), len(want))]
			if strings.Join(got, ",") != strings.Join(want, ",") {
				t.Fatalf("visited %v, want %v", got, want)
			}
			for i := len(want) - 2; i >= 0; i-- {
				m, _ = m.handleKey(keyMsg("["))
				if m.hunk == nil || fmt.Sprint(m.hunk.row) != want[i] {
					t.Fatalf("[ reached %+v, want row %s", m.hunk, want[i])
				}
			}
		})
	}
}

func TestContinuousFoldsAndSectionsKeepTheTopRow(t *testing.T) {
	m := continuousModel(t, 100, 14)
	fold := -1
	for row, r := range m.stream.rows {
		if r.session == 4 && r.kind == streamLine && m.stream.files[4][r.index].Kind == diff.DisplayFold {
			fold = row
			break
		}
	}
	m.scroll = fold - 2
	m.clampScroll()
	m.followScroll()
	anchor, _ := m.stream.anchor(m.scroll)
	m, _ = m.updateMouse(clickAt(m.fileListWidth()+1, m.contentTop()+fold-m.scroll))
	if !m.gapExpandedIn(4, m.stream.files[4][0].GapID) && len(m.expandedGaps[4]) == 0 {
		t.Fatal("click did not expand the fold of the clicked file")
	}
	if got, _ := m.stream.anchor(m.scroll); got != anchor {
		t.Fatalf("top row moved from %+v to %+v", anchor, got)
	}
	m, _ = m.handleKey(keyMsg("h"))
	if len(m.expandedGaps[4]) != 0 {
		t.Fatal("h did not collapse the visible gap")
	}

	m.selectSection(2)
	m, _ = m.handleKey(keyMsg("z"))
	if !m.collapsed[2] || m.stream.start[3] != m.stream.start[2]+2 {
		t.Fatal("z did not reduce the active section to its title")
	}
	if !strings.Contains(stripANSI(m.renderSectionTitle(2, 80)), "▸ /p/c.txt") {
		t.Fatal("collapsed title lacks its marker")
	}
	m, _ = m.updateMouse(clickAt(m.fileListWidth()+1, m.contentTop()+m.stream.start[2]-m.scroll))
	if m.collapsed[2] {
		t.Fatal("title click did not reopen the section")
	}
}

func TestContinuousPlanFollowsDirectionLayoutAndErrors(t *testing.T) {
	m := continuousModel(t, 220, 14, diff.Session{LocalPath: "/p/a", Result: stateFixture()}, diff.Session{LocalPath: "/p/b", Result: stateFixture()})
	m.selectSection(1)
	m.scroll += 4
	anchor, _ := m.stream.anchor(m.scroll)
	for _, key := range []string{" ", "v", "v", "v"} {
		m, _ = m.handleKey(keyMsg(key))
		if got, _ := m.stream.anchor(m.scroll); got.session != anchor.session || got.source != anchor.source {
			t.Fatalf("after %q the top row moved from %+v to %+v", key, anchor, got)
		}
	}
	if !m.stream.rows[m.stream.start[1]+1].kind.isLine() || m.stream.dirs[1] != m.syncDirs[1] {
		t.Fatal("plan did not follow the new preview direction")
	}
	m, _ = m.Update(MsgSyncError{SessionIdx: 1, Err: errors.New("upload failed")})
	if m.stream.rows[m.stream.start[1]+1].kind != streamNote || !strings.Contains(stripANSI(m.View()), "upload failed") {
		t.Fatal("plan kept the text diff of a failed file")
	}
}

func (k streamKind) isLine() bool { return k == streamLine }

func TestContinuousDecoratesVisibleFilesOneAtATime(t *testing.T) {
	sessions := make([]diff.Session, 3)
	for i := range sessions {
		sessions[i] = diff.Session{LocalPath: fmt.Sprintf("/p/%d.go", i), Result: shortNavigationResult()}
	}
	m := continuousModel(t, 100, 40, sessions...)
	cmd := m.Init()
	seen := map[*diff.DiffResult]bool{}
	for cmd != nil {
		msg := cmd().(MsgDecorated)
		if seen[msg.request.result] {
			t.Fatal("decorated the same file twice")
		}
		seen[msg.request.result] = true
		m, cmd = m.Update(msg)
	}
	for i := range sessions {
		if m.decorationFor(i) == nil {
			t.Fatalf("visible file %d was not decorated", i)
		}
	}
	m.decorated = nil
	first := m.startDecoration()
	request := m.decoration
	m.selectSection(2)
	m.scroll = m.totalLines()
	m.clampScroll()
	second := m.startDecoration()
	if second == nil || m.decoration == request {
		t.Fatal("a request for a file that left the screen was not replaced")
	}
	m, _ = m.Update(first())
	if m.decorationFor(0) != nil {
		t.Fatal("cancelled request stored data")
	}
}

func TestContinuousDecorationCacheIsBounded(t *testing.T) {
	sessions := make([]diff.Session, maxDecorations+8)
	for i := range sessions {
		sessions[i] = diff.Session{LocalPath: fmt.Sprintf("/p/%03d", i), Result: shortNavigationResult()}
	}
	m := continuousModel(t, 100, 12, sessions...)
	for i := range sessions {
		m.selectSection(i)
		if cmd := m.startDecoration(); cmd != nil {
			m, _ = m.Update(cmd())
		}
		if len(m.decorated) > maxDecorations+1 {
			t.Fatalf("cache grew to %d entries", len(m.decorated))
		}
	}
}

func TestContinuousViewGeometryAndSafety(t *testing.T) {
	sessions := []diff.Session{
		{LocalPath: "/p/\x1b[2Jevil.txt", Result: stateFixture()},
		{LocalPath: "/p/b.png", Result: &diff.DiffResult{Binary: true, ContentDiff: true}},
		{LocalPath: "/p/c.txt", Err: errors.New("bad\x1b]0;title\x07")},
	}
	for _, width := range []int{30, 39, 40, 80, 170, 240} {
		for _, layout := range []string{"unified", "split", "auto"} {
			m := continuousModel(t, width, 18, sessions...)
			m.SetPreferences(config.DiffPreferences{Layout: layout, Continuous: true})
			for step := 0; step < 3; step++ {
				view := m.View()
				if strings.Contains(view, "\x1b[2J") || strings.Contains(view, "\x1b]0;") {
					t.Fatal("raw escape sequence reached the terminal")
				}
				rows := strings.Split(view, "\n")
				if len(rows) != m.Height {
					t.Fatalf("width %d %s: %d rows", width, layout, len(rows))
				}
				for _, row := range rows {
					if lipgloss.Width(row) != width {
						t.Fatalf("width %d %s: row of %d cells: %q", width, layout, lipgloss.Width(row), stripANSI(row))
					}
				}
				m, _ = m.handleKey(keyMsg("pgdown"))
			}
		}
	}
}

func TestContinuousSnapshotRestoresModeFileAndLine(t *testing.T) {
	m := continuousModel(t, 100, 14)
	m.selectSection(4)
	m.scroll += 3
	anchor, _ := m.stream.anchor(m.scroll)
	snapshot := m.SnapshotView()

	restored := New(m.sessions, config.Host{}, nil, nil, 100, 14)
	restored.RestoreView(snapshot)
	if !restored.continuous() || restored.activeIdx != 4 {
		t.Fatalf("restored continuous=%v active=%d", restored.continuous(), restored.activeIdx)
	}
	if got, _ := restored.stream.anchor(restored.scroll); got != anchor {
		t.Fatalf("restored top %+v, want %+v", got, anchor)
	}
}

// A hunk header precedes its expanded leading context; [h] must still find the
// context below it.
func TestCollapseVisibleFoldBelowHunkHeader(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 14)
	m.setGapExpanded(0, true)
	m.prepareDisplayRows()
	m.scroll = 0
	if m.displayRows()[0].Kind != diff.DisplayHunkHeader {
		t.Fatal("fixture must start with the hunk header")
	}
	m, _ = m.handleKey(keyMsg("h"))
	if m.gapExpanded(0) {
		t.Fatal("h left the visible leading context expanded")
	}
}
