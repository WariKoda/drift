package diffview

import (
	"errors"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

func countsFixture(added, removed int) *diff.DiffResult {
	var lines []diff.DiffLine
	for i := 0; i < removed; i++ {
		lines = append(lines, diff.DiffLine{Text: "local", Kind: diff.LineRemoved, LocalNum: i + 1})
	}
	for i := 0; i < added; i++ {
		lines = append(lines, diff.DiffLine{Text: "remote", Kind: diff.LineAdded, RemoteNum: i + 1})
	}
	return &diff.DiffResult{Lines: lines}
}

func openTestRoot(t *testing.T) *fs.Root {
	t.Helper()
	root, err := fs.OpenRoot(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = root.Close() })
	return root
}

func sidebarText(m Model) []string {
	var rows []string
	for _, row := range m.renderFileListRows() {
		rows = append(rows, strings.TrimRight(stripANSI(row), " "))
	}
	return rows
}

func TestFileListGroupsProjectRelativeDirectories(t *testing.T) {
	root := openTestRoot(t)
	base := root.Base()
	sessions := []diff.Session{
		{LocalPath: filepath.Join(base, "README.md"), Result: countsFixture(6, 0)},
		{LocalPath: filepath.Join(base, "src", "a.go"), Result: countsFixture(1, 1)},
		{LocalPath: filepath.Join(base, "src", "gone.go"), Result: &diff.DiffResult{LocalOnly: true, Lines: countsFixture(0, 1).Lines}},
		{LocalPath: filepath.Join(base, "src", "new.go"), Result: &diff.DiffResult{RemoteOnly: true, Lines: countsFixture(2, 0).Lines}},
		{LocalPath: filepath.Join(base, "web", "logo.png"), Result: &diff.DiffResult{Binary: true, ContentDiff: true}},
		{LocalPath: filepath.Join(base, "web", "broken.css"), Err: errFixture("permission denied")},
	}
	m := New(sessions, config.Host{}, nil, root, 120, 14)
	m.syncDirs = make([]SyncDir, len(sessions))
	got := sidebarText(m)[:9]
	want := []string{
		"[/] ./",
		"▶ ~ README.md                       +6 —",
		"[/] src/",
		"  ~ a.go                         +1 -1 —",
		"  L gone.go                         -1 —",
		"  R new.go                          +2 —",
		"[/] web/",
		"  B logo.png                           —",
		"  ! broken.css                         —",
	}
	if !slices.Equal(got, want) {
		t.Fatalf("sidebar:\n%s\nwant:\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
	for _, row := range m.renderFileListRows() {
		if lipgloss.Width(row) != m.fileListWidth() {
			t.Fatalf("row width %d, want %d: %q", lipgloss.Width(row), m.fileListWidth(), row)
		}
	}
}

func TestFileListFallsBackToRemoteRootThenFullPath(t *testing.T) {
	m := New([]diff.Session{
		{LocalPath: "/elsewhere/x.txt", RemotePath: "/srv/site/assets/x.txt", Result: countsFixture(1, 0)},
		{LocalPath: "/elsewhere/y.txt", RemotePath: "/other/y.txt", Result: countsFixture(1, 0)},
	}, config.Host{RootPath: "/srv/site/"}, nil, nil, 120, 10)
	if got := m.displayPath(&m.sessions[0]); got != "assets/x.txt" {
		t.Fatalf("remote-relative path = %q", got)
	}
	if got := m.displayPath(&m.sessions[1]); got != "/elsewhere/y.txt" {
		t.Fatalf("unrelated path = %q, want the full local path", got)
	}
}

func TestFileListCountsFollowPreviewDirection(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a.txt", Result: countsFixture(3, 1)}}, config.Host{}, nil, nil, 120, 10)
	for _, tc := range []struct {
		dir  SyncDir
		want string
	}{
		{DirNone, "+3 -1"},
		{DirDownload, "+3 -1"},
		{DirDeleteLocal, "+3 -1"},
		{DirUpload, "+1 -3"},
		{DirDeleteRemote, "+1 -3"},
	} {
		m.syncDirs[0] = tc.dir
		if got := stripANSI(m.countsLabel(0)); got != tc.want {
			t.Fatalf("direction %d: counts %q, want %q", tc.dir, got, tc.want)
		}
		if title := stripANSI(m.renderFileTitle(0, 100)); !strings.Contains(title, tc.want) {
			t.Fatalf("direction %d: title %q does not match the file list %q", tc.dir, title, tc.want)
		}
	}
}

func TestFileListRowsStayInsideNarrowSidebar(t *testing.T) {
	name := "a-very-long-name-\x1b[31m-with-escape.txt"
	m := New([]diff.Session{{LocalPath: "/p/" + name, Result: countsFixture(12345, 6789)}}, config.Host{}, nil, nil, 40, 10)
	width := m.fileListWidth()
	row := m.renderFileRow(0, name, width)
	plain := stripANSI(row)
	if lipgloss.Width(row) != width || strings.Contains(row, "\x1b[31m") {
		t.Fatalf("row escaped the sidebar or kept a raw escape: %q", row)
	}
	if strings.Contains(plain, "+12345") {
		t.Fatalf("counts left no room for the name: %q", plain)
	}
	if !strings.Contains(plain, "a-very") {
		t.Fatalf("name missing: %q", plain)
	}
}

func TestFileListShowsHeadingWhenScrollingToGroupStart(t *testing.T) {
	var sessions []diff.Session
	for _, dir := range []string{"a", "b", "c", "d"} {
		for _, file := range []string{"1", "2", "3", "4", "5"} {
			sessions = append(sessions, diff.Session{LocalPath: "/p/" + dir + "/" + file, Result: countsFixture(1, 0)})
		}
	}
	m := New(sessions, config.Host{}, nil, nil, 100, 10)
	for range sessions {
		m, _ = m.handleKey(keyMsg("n"))
	}
	if m.fileListOffset == 0 {
		t.Fatal("file list did not follow the cursor down")
	}
	for m.activeIdx > 10 {
		m, _ = m.handleKey(keyMsg("p"))
	}
	list := m.fileList()
	if heading := list.rowOf[10] - 1; list.rows[heading].dir != "/p/c" || m.fileListOffset != heading {
		t.Fatalf("offset %d does not show the heading of the selected group at row %d", m.fileListOffset, heading)
	}
}

func TestFileListClicksSelectFilesNotHeadings(t *testing.T) {
	m := New([]diff.Session{
		{LocalPath: "/p/a/1", Result: countsFixture(1, 0)},
		{LocalPath: "/p/b/2", Result: countsFixture(1, 0)},
	}, config.Host{}, nil, nil, 100, 12)
	click := func(row int) {
		m, _ = m.updateMouse(clickAt(1, bodyTop+row))
	}
	click(2) // heading of /p/b
	if m.activeIdx != 0 {
		t.Fatalf("heading click selected file %d", m.activeIdx)
	}
	click(3)
	if m.activeIdx != 1 {
		t.Fatalf("file click selected %d, want 1", m.activeIdx)
	}
}

func TestFileListWidthKeysResizeForSessionOnly(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}}, config.Host{}, nil, nil, 120, 12)
	start := m.fileListWidth()
	m, _ = m.handleKey(keyMsg(">"))
	if m.fileListWidth() != start+fileListStep || m.fileListWidth()+1+m.diffWidth() != m.Width {
		t.Fatalf("width %d after [>], want %d", m.fileListWidth(), start+fileListStep)
	}
	for range 40 {
		m, _ = m.handleKey(keyMsg(">"))
	}
	if m.diffWidth() != 20 {
		t.Fatalf("diff pane shrank to %d columns, want the 20-column minimum", m.diffWidth())
	}
	for range 40 {
		m, _ = m.handleKey(keyMsg("<"))
	}
	if m.fileListWidth() != 16 {
		t.Fatalf("sidebar shrank to %d, want 16", m.fileListWidth())
	}
	m, _ = m.handleKey(keyMsg(">"))
	if m.fileListWidth() != 16+fileListStep {
		t.Fatalf("[>] after hitting the minimum moved to %d", m.fileListWidth())
	}
	for _, row := range strings.Split(m.View(), "\n") {
		if lipgloss.Width(row) != m.Width {
			t.Fatalf("view row width %d, want %d", lipgloss.Width(row), m.Width)
		}
	}
	m.SetSize(30, 12)
	m, _ = m.handleKey(keyMsg(">"))
	if m.fileListWidth() != 0 {
		t.Fatal("hidden sidebar reappeared")
	}
}

func errFixture(text string) error { return errors.New(text) }

func clickAt(x, y int) tea.MouseMsg {
	return tea.MouseMsg{X: x, Y: y, Button: tea.MouseButtonLeft, Action: tea.MouseActionPress}
}
