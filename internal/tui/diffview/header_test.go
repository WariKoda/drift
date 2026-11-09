package diffview

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/charmbracelet/lipgloss"
)

func TestFileTitleShowsRelativePathStatusAndCounts(t *testing.T) {
	root := openTestRoot(t)
	m := New([]diff.Session{{
		LocalPath:  filepath.Join(root.Base(), "src", "new.go"),
		RemotePath: "/srv/src/new.go",
		Result:     &diff.DiffResult{RemoteOnly: true, Lines: countsFixture(3, 0).Lines},
	}}, config.Host{}, nil, root, 140, 20)
	m.syncDirs[0] = DirDownload
	title := strings.TrimSpace(stripANSI(m.renderFileTitle(m.activeIdx, m.diffWidth())))
	if !strings.HasPrefix(title, "src/new.go · remote only") || !strings.HasSuffix(title, "+3  Local → Remote (before → after)") {
		t.Fatalf("title = %q", title)
	}
}

func TestFileTitleFitsEveryWidth(t *testing.T) {
	m := New([]diff.Session{{
		LocalPath: "/project/some/deeply/nested/directory/with-a-long-file-name.php",
		Result:    countsFixture(120, 45),
	}}, config.Host{}, nil, nil, 140, 20)
	m.syncDirs[0] = DirUpload
	for width := 1; width <= 140; width++ {
		row := m.renderFileTitle(m.activeIdx, width)
		if got := lipgloss.Width(row); got != width {
			t.Fatalf("width %d rendered %d cells: %q", width, got, stripANSI(row))
		}
		plain := stripANSI(row)
		if width >= 40 && !strings.Contains(plain, "Remote → Local") {
			t.Fatalf("width %d dropped the before/after orientation: %q", width, plain)
		}
		if width >= 80 && !strings.Contains(plain, "+45 -120") {
			t.Fatalf("width %d dropped the counts: %q", width, plain)
		}
		if width >= 40 && !strings.Contains(plain, "name.php") {
			t.Fatalf("width %d lost the file name: %q", width, plain)
		}
	}
}

func TestFullPathsToggleKeepsGeometry(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/local/a\x1b[2Jb", RemotePath: "/remote/a", Result: stateFixture()}}, config.Host{}, nil, nil, 120, 20)
	m, _ = m.handleKey(keyMsg("G"))
	end, viewport := m.scroll, m.viewportHeight()
	m, _ = m.handleKey(keyMsg("f"))
	if m.chromeRows() != pathChrome+pathRows || m.viewportHeight() != viewport-pathRows || m.scroll != end {
		t.Fatalf("chrome %d viewport %d scroll %d after showing paths", m.chromeRows(), m.viewportHeight(), m.scroll)
	}
	view := m.View()
	if strings.Contains(view, "\x1b[2J") {
		t.Fatal("full path row wrote a raw escape sequence")
	}
	rows := strings.Split(view, "\n")
	if len(rows) != m.Height {
		t.Fatalf("view has %d rows, want %d", len(rows), m.Height)
	}
	for _, want := range []string{"Local  /local/a", "Remote /remote/a"} {
		if !strings.Contains(stripANSI(view), want) {
			t.Fatalf("view lacks %q", want)
		}
	}
	if hit := m.hitTest(m.fileListWidth()+1, m.contentTop()); hit.zone != zoneContent || hit.index != m.scroll {
		t.Fatalf("first content row hit = %+v, want scroll %d", hit, m.scroll)
	}
	if hit := m.hitTest(m.fileListWidth()+1, m.contentTop()-1); hit.zone != zoneNone {
		t.Fatal("separator below the paths is clickable content")
	}
	m, _ = m.handleKey(keyMsg("f"))
	if m.chromeRows() != pathChrome || m.scroll != end {
		t.Fatalf("hiding paths left chrome %d scroll %d, want %d", m.chromeRows(), m.scroll, end)
	}
}
