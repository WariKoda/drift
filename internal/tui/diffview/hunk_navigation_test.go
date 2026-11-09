package diffview

import (
	"errors"
	"fmt"
	"reflect"
	"slices"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	tea "github.com/charmbracelet/bubbletea"
)

func shortNavigationResult() *diff.DiffResult {
	return &diff.DiffResult{Lines: []diff.DiffLine{
		{Text: "first", Kind: diff.LineRemoved, LocalNum: 1},
		{Text: "same", Kind: diff.LineEqual, LocalNum: 2, RemoteNum: 1},
		{Text: "last", Kind: diff.LineAdded, RemoteNum: 2},
	}}
}

func navigationHeaders(m Model) []int {
	var headers []int
	for i, row := range m.displayRows() {
		if row.Kind == diff.DisplayHunkHeader {
			headers = append(headers, i)
		}
	}
	return headers
}

func TestHunkNavigationCrossesFilesInBothDirections(t *testing.T) {
	for _, layout := range []string{"unified", "split", "auto"} {
		for _, direction := range []SyncDir{DirNone, DirUpload, DirDownload, DirDeleteLocal, DirDeleteRemote} {
			t.Run(fmt.Sprintf("%s/direction=%d", layout, direction), func(t *testing.T) {
				m := New([]diff.Session{{LocalPath: "a", Result: stateFixture()}, {LocalPath: "b", Result: stateFixture()}}, config.Host{}, nil, nil, 220, 12)
				m.SetPreferences(config.DiffPreferences{Layout: layout})
				m.syncDirs = []SyncDir{direction, direction}
				m.prepareDisplayRows()
				m.toggleAllFolds()
				headers := navigationHeaders(m)
				if len(headers) != 2 {
					t.Fatalf("headers=%v", headers)
				}
				m.scroll = headers[0]
				folds := make(map[int]struct{})
				for gap := range m.expandedFor(0) {
					folds[gap] = struct{}{}
				}
				m.horizontal = 8
				m, _ = m.handleKey(keyMsg("]"))
				if m.activeIdx != 0 || m.scroll != headers[1] {
					t.Fatal("skipped a hunk in the current file")
				}
				m, _ = m.handleKey(keyMsg("]"))
				if m.activeIdx != 1 || m.scroll != navigationHeaders(m)[0] {
					t.Fatal("did not enter the next file at its first hunk")
				}
				m, _ = m.handleKey(keyMsg("["))
				if m.activeIdx != 0 || m.scroll != headers[1] {
					t.Fatal("did not enter the previous file at its last hunk")
				}
				if m.horizontal != 8 || !reflect.DeepEqual(folds, m.expandedFor(0)) {
					t.Fatal("file crossing lost horizontal position or folds")
				}
				m, _ = m.handleKey(keyMsg("["))
				if m.scroll != headers[0] {
					t.Fatal("previous hunk skipped within the file")
				}
				m, _ = m.handleKey(keyMsg("["))
				if m.activeIdx != 0 || m.scroll != headers[0] {
					t.Fatal("navigation wrapped before the first file")
				}
				if !slices.Equal(m.syncDirs, []SyncDir{direction, direction}) {
					t.Fatal("navigation changed sync decisions")
				}
			})
		}
	}
}

func TestHunkNavigationAdvancesWhenViewportCannotMove(t *testing.T) {
	for _, layout := range []string{"unified", "split"} {
		t.Run(layout, func(t *testing.T) {
			result := shortNavigationResult()
			m := New([]diff.Session{{LocalPath: "a", Result: result}, {LocalPath: "b", Result: result}}, config.Host{}, nil, nil, 220, 100)
			m.SetPreferences(config.DiffPreferences{Layout: layout})
			for step, wantFile := range []int{0, 1, 1, 1} {
				m, _ = m.handleKey(keyMsg("]"))
				if m.activeIdx != wantFile || m.scroll != 0 {
					t.Fatalf("forward step %d: file=%d scroll=%d", step, m.activeIdx, m.scroll)
				}
			}
			for step, wantFile := range []int{1, 0, 0, 0} {
				m, _ = m.handleKey(keyMsg("["))
				if m.activeIdx != wantFile || m.scroll != 0 {
					t.Fatalf("backward step %d: file=%d scroll=%d", step, m.activeIdx, m.scroll)
				}
			}
		})
	}
}

func TestHunkNavigationSkipsSessionsWithoutRenderableHunks(t *testing.T) {
	unavailable := []diff.Session{
		{Result: &diff.DiffResult{Lines: []diff.DiffLine{{Text: "identical"}}}},
		{},
		{Err: errors.New("cannot compare"), Result: stateFixture()},
		{Result: &diff.DiffResult{Binary: true, ContentDiff: true, Lines: stateFixture().Lines}},
		{Result: &diff.DiffResult{ContentDiff: true}},
	}
	sessions := append([]diff.Session(nil), unavailable...)
	first := len(sessions)
	sessions = append(sessions, diff.Session{LocalPath: "a", Result: stateFixture()})
	sessions = append(sessions, unavailable...)
	last := len(sessions)
	sessions = append(sessions, diff.Session{LocalPath: "b", Result: stateFixture()})
	sessions = append(sessions, unavailable...)
	m := New(sessions, config.Host{}, nil, nil, 100, 12)
	m, _ = m.handleKey(keyMsg("]"))
	if m.activeIdx != first {
		t.Fatalf("file=%d want %d", m.activeIdx, first)
	}
	m.toggleAllFolds()
	headers := navigationHeaders(m)
	m.scroll = headers[len(headers)-1]
	m, _ = m.handleKey(keyMsg("]"))
	if m.activeIdx != last {
		t.Fatalf("file=%d want %d", m.activeIdx, last)
	}
	if row := m.fileList().rowOf[last]; row < m.fileListOffset || row >= m.fileListOffset+m.fileListHeight() {
		t.Fatal("file list did not follow the selected hunk's file")
	}
	m.toggleAllFolds()
	headers = navigationHeaders(m)
	m.scroll = headers[len(headers)-1]
	m, _ = m.handleKey(keyMsg("]"))
	if m.activeIdx != last {
		t.Fatal("selected a trailing non-hunk session")
	}
	m.scroll = navigationHeaders(m)[0]
	m, _ = m.handleKey(keyMsg("["))
	if m.activeIdx != first {
		t.Fatal("did not skip non-hunk sessions backwards")
	}
	m.scroll = navigationHeaders(m)[0]
	m, _ = m.handleKey(keyMsg("["))
	if m.activeIdx != first {
		t.Fatal("selected a leading non-hunk session")
	}
}

func TestHunkNavigationUsesEndpointsInsteadOfSavedReadingPosition(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "a", Result: stateFixture()}, {LocalPath: "b", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 12)
	m.toggleAllFolds()
	m.selectFile(1)
	m.toggleAllFolds()
	headers := navigationHeaders(m)
	m.scroll = headers[len(headers)-1]
	m.selectFile(0)
	headers = navigationHeaders(m)
	m.scroll = headers[len(headers)-1]
	m, _ = m.handleKey(keyMsg("]"))
	if m.activeIdx != 1 || m.scroll != navigationHeaders(m)[0] {
		t.Fatal("next hunk restored a visited file's old reading position")
	}
	m.selectFile(0)
	m.scroll = navigationHeaders(m)[0]
	m.selectFile(1)
	m.scroll = navigationHeaders(m)[0]
	m, _ = m.handleKey(keyMsg("["))
	headers = navigationHeaders(m)
	if m.activeIdx != 0 || m.scroll != headers[len(headers)-1] {
		t.Fatal("previous hunk restored a visited file's old reading position")
	}
}

func TestManualScrollResetsClampedHunkNavigation(t *testing.T) {
	for _, method := range []string{"home", "down", "pgdown", "wheel"} {
		t.Run(method, func(t *testing.T) {
			m := New([]diff.Session{{LocalPath: "a", Result: shortNavigationResult()}, {LocalPath: "b", Result: shortNavigationResult()}}, config.Host{}, nil, nil, 100, 100)
			m, _ = m.handleKey(keyMsg("]"))
			if m.hunk == nil || m.hunk.row != navigationHeaders(m)[1] {
				t.Fatal("last hunk not reached")
			}
			if method == "wheel" {
				m, _ = m.updateMouse(tea.MouseMsg{Button: tea.MouseButtonWheelDown, X: m.fileListWidth() + 1, Y: m.contentTop()})
			} else {
				m, _ = m.handleKey(keyMsg(method))
			}
			m, _ = m.handleKey(keyMsg("]"))
			if m.activeIdx != 0 {
				t.Fatal("manual scroll left the old logical hunk active")
			}
		})
	}
}

func TestHunkNavigationInvalidatesTargetWithDisplayPlan(t *testing.T) {
	m := New([]diff.Session{{Result: shortNavigationResult()}}, config.Host{}, nil, nil, 220, 100)
	m, _ = m.handleKey(keyMsg("]"))
	m.SetPreferences(config.DiffPreferences{Layout: "split"})
	if m.hunk != nil {
		t.Fatal("layout change retained an old display plan through the hunk target")
	}
	m, _ = m.handleKey(keyMsg("]"))
	m.sessions[0].Result = stateFixture()
	m.prepareDisplayRows()
	if m.hunk != nil {
		t.Fatal("comparison replacement retained the old hunk target")
	}
	m.toggleAllFolds()
	m.scroll = navigationHeaders(m)[0]
	m, _ = m.handleKey(keyMsg("]"))
	m.toggleAllFolds()
	if m.hunk != nil {
		t.Fatal("fold change retained the old hunk target")
	}
}

func TestHunkNavigationWithoutAnyHunksIsStable(t *testing.T) {
	for _, sessions := range [][]diff.Session{
		nil,
		{{Result: &diff.DiffResult{Lines: []diff.DiffLine{{Text: "equal"}}}}},
		{{Result: &diff.DiffResult{Binary: true, ContentDiff: true}}, {Err: errors.New("cannot compare")}},
	} {
		m := New(sessions, config.Host{}, nil, nil, 100, 24)
		for _, key := range []string{"]", "]", "[", "["} {
			m, _ = m.handleKey(keyMsg(key))
			if m.activeIdx != 0 || m.scroll != 0 {
				t.Fatal("empty hunk navigation changed state")
			}
		}
	}
}

func TestHunkFileCrossingReplacesDecorationRequest(t *testing.T) {
	result := &diff.DiffResult{Lines: []diff.DiffLine{{Text: "changed", Kind: diff.LineRemoved, LocalNum: 1}}}
	m := New([]diff.Session{{LocalPath: "a", Result: result}, {LocalPath: "b", Result: &diff.DiffResult{Lines: result.Lines}}}, config.Host{}, nil, nil, 100, 24)
	defer func() { m.Close() }()
	old := m.Init()
	m, cmd := m.Update(keyMsg("]"))
	if m.activeIdx != 1 || cmd == nil {
		t.Fatal("file crossing did not start presentation work for the new file")
	}
	request := m.decoration
	m, _ = m.Update(old())
	if m.decoration != request || m.columnsReady {
		t.Fatal("old presentation result affected the next file")
	}
	m, _ = m.Update(cmd())
	if !m.columnsReady || m.decorationError != "" {
		t.Fatal("next file's presentation result was not accepted")
	}
}
