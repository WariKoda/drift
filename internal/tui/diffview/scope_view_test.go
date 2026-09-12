package diffview

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
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
	for _, want := range []string{"12 pairs", "2 hidden", "3 ignored skipped", "1 ignored dirs skipped", "1 fixed excluded", "include ignored: off"} {
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
