package diffview

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/styles"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

func TestDirectionSwitchUpdatesEntirePreview(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	for _, key := range []string{" ", "A"} {
		t.Run(key, func(t *testing.T) {
			m := Model{
				conn: connectDiffTestHost(t, startFTPTestServer(t, 1).host(t)),
				sessions: []diff.Session{{
					LocalPath: "/local/file", RemotePath: "/remote/file",
					Result: &diff.DiffResult{Lines: []diff.DiffLine{
						{Kind: diff.LineRemoved, Text: "L1", LocalNum: 1},
						{Kind: diff.LineRemoved, Text: "L2", LocalNum: 2},
						{Kind: diff.LineAdded, Text: "R", RemoteNum: 1},
					}},
				}},
				syncDirs: []SyncDir{DirNone}, Width: 140, Height: 20,
			}
			for _, tt := range []struct {
				direction                       SyncDir
				legend, header, first, leftPath string
			}{
				{DirUpload, "Line numbers: Remote → Local (before → after)", "@@ -1,1 +1,2 @@", "   1      - R", "/remote/file"},
				{DirDownload, "Line numbers: Local → Remote (before → after)", "@@ -1,2 +1,1 @@", "   1      - L1", "/local/file"},
				{DirNone, "No action · Compare only · Lines: Local → Remote", "@@ -1,2 +1,1 @@", "   1        L1", "/local/file"},
			} {
				m, _ = m.handleKey(keyMsg(key))
				if m.syncDirs[0] != tt.direction {
					t.Fatalf("direction = %v, want %v", m.syncDirs[0], tt.direction)
				}
				rows := m.renderDiffPaneRows(m.activeSession())
				if !strings.HasPrefix(stripANSI(rows[0]), tt.leftPath) {
					t.Fatalf("path order = %q", rows[0])
				}
				if strings.TrimSpace(stripANSI(rows[1])) != tt.legend {
					t.Fatalf("legend = %q, want %q", rows[1], tt.legend)
				}
				if !strings.Contains(stripANSI(rows[pathChrome]), tt.header) {
					t.Fatalf("header = %q, want %q", rows[pathChrome], tt.header)
				}
				if got := strings.TrimRight(stripANSI(rows[pathChrome+1]), " "); got != tt.first {
					t.Fatalf("first changed row = %q, want %q", got, tt.first)
				}
				if tt.direction == DirNone {
					for i, text := range []string{"L1", "L2", "R"} {
						gutter := []string{"   1      ", "   2      ", "        1 "}[i]
						want := styles.Muted.Render(gutter) + styles.File.Bold(true).Render(" ") +
							styles.File.Render(" "+text+strings.Repeat(" ", m.diffWidth()-12-len(text))) + styles.File.Render("")
						if rows[pathChrome+1+i] != want {
							t.Fatalf("no-action row must use neutral colours without a change marker: %q", rows[pathChrome+1+i])
						}
					}
				}
				if len(strings.Split(m.View(), "\n")) != m.Height {
					t.Fatal("legend changed terminal height")
				}
				if hit := m.hitTest(m.fileListWidth()+1, bodyTop+1); hit.zone != zoneNone {
					t.Fatal("legend is clickable diff content")
				}
			}
		})
	}
}

func TestUploadFoldClickKeepsScrolledSourceLine(t *testing.T) {
	lines := []diff.DiffLine{
		{Kind: diff.LineRemoved, Text: "L1", LocalNum: 1},
		{Kind: diff.LineRemoved, Text: "L2", LocalNum: 2},
		{Kind: diff.LineAdded, Text: "R", RemoteNum: 1},
	}
	for i := 0; i < 20; i++ {
		lines = append(lines, diff.DiffLine{Text: "same", LocalNum: i + 3, RemoteNum: i + 2})
	}
	m := Model{
		conn:     connectDiffTestHost(t, startFTPTestServer(t, 1).host(t)),
		sessions: []diff.Session{{Result: &diff.DiffResult{Lines: lines}}},
		syncDirs: []SyncDir{DirNone}, Width: 120, Height: headerLines + footerLines + pathChrome + 7,
	}
	m, _ = m.handleKey(keyMsg(" "))
	m.scroll = 1
	anchor := m.displayRows()[m.scroll].SourceLine()
	foldAt := -1
	for i, row := range m.displayRows() {
		if row.Kind == diff.DisplayFold {
			foldAt = i
		}
	}
	if foldAt < m.scroll || foldAt >= m.scroll+m.viewportHeight() {
		t.Fatal("fixture fold is outside the viewport")
	}
	m, _ = m.updateMouse(tea.MouseMsg{
		X: m.fileListWidth() + 1, Y: m.contentTop() + foldAt - m.scroll,
		Button: tea.MouseButtonLeft, Action: tea.MouseActionPress,
	})
	if !m.gapExpanded(3) {
		t.Fatal("click did not expand the original gap")
	}
	if got := m.displayRows()[m.scroll].SourceLine(); got != anchor {
		t.Fatalf("expanded view lost source anchor %d, got %d", anchor, got)
	}
	m.toggleAllFolds()
	if m.gapExpanded(3) || m.displayRows()[m.scroll].SourceLine() != anchor {
		t.Fatal("collapse lost fold state or source anchor")
	}
}

func TestDeletePreviewShowsRemovalOnEitherSide(t *testing.T) {
	for _, tt := range []struct {
		dir    SyncDir
		result diff.DiffResult
	}{
		{DirDeleteLocal, diff.DiffResult{LocalOnly: true, Lines: []diff.DiffLine{{Kind: diff.LineRemoved, LocalNum: 1, Text: "gone"}}}},
		{DirDeleteRemote, diff.DiffResult{RemoteOnly: true, Lines: []diff.DiffLine{{Kind: diff.LineAdded, RemoteNum: 1, Text: "gone"}}}},
	} {
		m := Model{
			sessions: []diff.Session{{Result: &tt.result}}, syncDirs: []SyncDir{tt.dir},
			Width: 120, Height: 20,
		}
		rows := m.renderDiffPaneRows(m.activeSession())
		if !strings.Contains(stripANSI(rows[pathChrome]), "@@ -1,1 +0,0 @@") {
			t.Fatalf("delete header = %q", rows[pathChrome])
		}
		if got := strings.TrimRight(stripANSI(rows[pathChrome+1]), " "); got != "   1      - gone" {
			t.Fatalf("delete row = %q", got)
		}
	}
}
