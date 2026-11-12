package diffview

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/ftptest"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
	tea "github.com/charmbracelet/bubbletea"
)

func TestScopeSummaryAndToggle(t *testing.T) {
	model := New(nil, testHost(), nil, nil, 180, 24)
	model.SetScope(syncpolicy.ScopeSummary{
		Pairs:                   12,
		Hidden:                  2,
		IgnoredFilesSkipped:     3,
		IgnoredDirsSkipped:      1,
		HardExcludedSkipped:     1,
		ExplicitIgnoredIncluded: 1,
	}, syncpolicy.ScopeOptions{})
	status := model.scopeSummaryLabel()
	for _, want := range []string{"12 pairs", "2 hidden", "3 ignored skipped", "1 ignored dirs skipped", "1 fixed excluded"} {
		if !strings.Contains(status, want) {
			t.Fatalf("status %q does not contain %q", status, want)
		}
	}

	_, cmd := model.handleKey(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("i")})
	if cmd == nil {
		t.Fatal("ignored toggle did not request a scope reload")
	}
	msg, ok := cmd().(MsgScopeReloadRequested)
	if !ok || !msg.IncludeIgnored {
		t.Fatalf("toggle result = %#v", cmd())
	}
}

func testHost() config.Host { return config.Host{Name: "test"} }

func TestPlanSummaryTracksActionsWithoutStartingSync(t *testing.T) {
	m := New([]diff.Session{
		{Result: countsFixture(1, 1)}, {Result: countsFixture(1, 1)},
		{Result: countsFixture(1, 1)}, {Result: countsFixture(1, 1)},
	}, testHost(), connectDiffTestHost(t, ftptest.Start(t, 1).Host(t)), nil, 300, 24)
	m.syncDirs = []SyncDir{DirUpload, DirDownload, DirDeleteLocal, DirDeleteRemote}
	status := stripANSI(m.renderStatus(m.activeSession()))
	for _, want := range []string{"Planned: ", "1 upload", "1 download", "1 delete Local", "1 delete Remote"} {
		if !strings.Contains(status, want) {
			t.Fatalf("plan lacks %q: %q", want, status)
		}
	}
	m, cmd := m.handleKey(tea.KeyMsg{Type: tea.KeySpace})
	if cmd != nil || m.syncing || m.quickSyncing {
		t.Fatal("changing the plan started work")
	}
	status = stripANSI(m.renderStatus(m.activeSession()))
	if strings.Contains(status, "1 upload") || !strings.Contains(status, "2 download") {
		t.Fatalf("plan did not follow action change: %q", status)
	}
	if strings.Contains(status, "0 hidden") || strings.Contains(status, "ignored: off") {
		t.Fatalf("plan contains inactive scope counters: %q", status)
	}
}
