package diffview

import (
	"fmt"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/ftptest"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestErrorDetailsCaptureHiddenActions(t *testing.T) {
	conn := connectDiffTestHost(t, ftptest.Start(t, 1).Host(t))
	for _, key := range []string{" ", "A", "s", "S", "u", "d", "i", "r", "tab", "enter", "c"} {
		t.Run(key, func(t *testing.T) {
			m := New([]diff.Session{{Result: &diff.DiffResult{ContentDiff: true}}}, config.Host{}, conn, nil, 100, 24)
			m.SetSyncResult("failed", []syncpolicy.Failure{{Reason: "denied"}})
			before := m.syncDirs[0]
			m, cmd := m.Update(keyMsg(key))
			if cmd != nil || m.remoteBusy() || m.syncDirs[0] != before || !m.showErrors {
				t.Fatalf("key %q acted behind error details", key)
			}
		})
	}
}

func TestErrorDetailsScrollEveryFailureAndLongReason(t *testing.T) {
	m := New(nil, config.Host{}, nil, nil, 100, 14)
	failures := make([]syncpolicy.Failure, 25)
	for i := range failures {
		failures[i] = syncpolicy.Failure{Path: fmt.Sprintf("/file-%02d", i), Operation: "upload", Reason: "denied"}
	}
	failures[24].Reason = strings.Repeat("long reason ", 20) + "LAST-ERROR-MARKER"
	m.SetSyncResult("25 errors", failures)
	for i := 0; i < 200; i++ {
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	}
	if !strings.Contains(m.View(), "LAST-ERROR-MARKER") {
		t.Fatal("last error is not reachable")
	}
	for _, line := range strings.Split(m.View(), "\n") {
		if ansi.StringWidth(line) > m.Width {
			t.Fatal("error details overflow terminal")
		}
	}
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyHome})
	if m.errorOffset != 0 {
		t.Fatal("Home did not return to first error")
	}
	m, _ = m.Update(tea.MouseMsg{X: m.fileListWidth() + 2, Y: m.contentTop() + 1, Button: tea.MouseButtonWheelDown})
	if m.errorOffset == 0 || m.scroll != 0 {
		t.Fatal("wheel did not scroll errors independently of the diff")
	}
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnd})
	m.SetSize(160, 40)
	if m.errorOffset > max(0, len(m.errorRows(m.diffWidth()))-max(1, m.viewportHeight()-1)) {
		t.Fatal("resize did not clamp the error viewport")
	}
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if m.showErrors {
		t.Fatal("Escape did not close error details")
	}
}

func TestComparisonEscapesPathsAndFailureControls(t *testing.T) {
	unsafe := "/file\n\x1b]52;c;SGVsbG8=\a"
	m := New([]diff.Session{{LocalPath: unsafe, RemotePath: unsafe, Result: &diff.DiffResult{ContentDiff: true}}}, config.Host{Name: unsafe}, nil, nil, 160, 24)
	for _, show := range []bool{false, true} {
		m.SetSyncResult("failed", []syncpolicy.Failure{{Path: unsafe, Reason: unsafe}})
		m.showErrors = show
		if strings.Contains(m.View(), "\x1b]52;") || strings.Count(m.View(), "\n")+1 != m.Height {
			t.Fatal("comparison emitted terminal controls")
		}
	}
}
