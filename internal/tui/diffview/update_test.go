package diffview

import (
	"errors"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
	tea "github.com/charmbracelet/bubbletea"
)

func TestQuickSyncBlocksOtherRemoteActions(t *testing.T) {
	model := Model{
		sessions: []diff.Session{{
			LocalPath:  "/local/file.txt",
			RemotePath: "/remote/file.txt",
			Result:     &diff.DiffResult{ContentDiff: true},
		}},
		syncDirs: []SyncDir{DirUpload},
	}

	model, cmd := model.handleKey(keyMsg("u"))
	if cmd == nil {
		t.Fatal("quick upload did not return a command")
	}
	if !model.quickSyncing {
		t.Fatal("quick upload did not mark the remote connection busy")
	}

	for _, key := range []string{"u", "d", "r", "s", "S", "q", "esc"} {
		t.Run(key, func(t *testing.T) {
			next, cmd := model.handleKey(keyMsg(key))
			if cmd != nil {
				t.Fatalf("key %q started another remote command during quick sync", key)
			}
			if !next.quickSyncing {
				t.Fatalf("key %q cleared the quick-sync state", key)
			}
		})
	}
}

func TestQuickSyncErrorReleasesRemoteActions(t *testing.T) {
	model := Model{
		quickSyncing: true,
		sessions:     []diff.Session{{Result: &diff.DiffResult{ContentDiff: true}}},
		syncDirs:     []SyncDir{DirUpload},
	}

	model, _ = model.Update(MsgSyncError{Err: errors.New("transfer failed")})
	if model.quickSyncing {
		t.Fatal("quick-sync state remained active after an error")
	}
	if model.sessions[0].Err == nil {
		t.Fatal("quick-sync error was not recorded on the active session")
	}
}

func TestBulkSyncAndRefreshBlockQuickSync(t *testing.T) {
	base := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true},
		}},
		syncDirs: []SyncDir{DirUpload},
	}

	for _, test := range []struct {
		name  string
		model Model
	}{
		{name: "bulk sync", model: func() Model {
			model := base
			model.syncing = true
			return model
		}()},
		{name: "refresh", model: func() Model {
			model := base
			model.refreshing = true
			return model
		}()},
	} {
		t.Run(test.name, func(t *testing.T) {
			next, cmd := test.model.handleKey(keyMsg("u"))
			if cmd != nil {
				t.Fatal("quick upload started while another remote action was active")
			}
			if next.quickSyncing {
				t.Fatal("blocked quick upload marked itself active")
			}
		})
	}
}

func TestDiffScrollNavigation(t *testing.T) {
	lines := make([]diff.DiffLine, 20)
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 12, // one file-list row leaves a four-line diff viewport
	}

	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyRunes, Runes: []rune("j")},
		{Type: tea.KeyDown},
		{Type: tea.KeyRunes, Runes: []rune("J")},
	} {
		model, _ = model.handleKey(key)
	}
	if model.scroll != 3 {
		t.Fatalf("line scrolling set offset to %d, want 3", model.scroll)
	}

	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyRunes, Runes: []rune("k")},
		{Type: tea.KeyUp},
		{Type: tea.KeyRunes, Runes: []rune("K")},
		{Type: tea.KeyUp},
	} {
		model, _ = model.handleKey(key)
	}
	if model.scroll != 0 {
		t.Fatalf("upward scrolling set offset to %d, want clamped offset 0", model.scroll)
	}
}

func TestDiffPageAndBoundaryNavigation(t *testing.T) {
	lines := make([]diff.DiffLine, 20)
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 12, // one file-list row leaves a four-line diff viewport
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyPgDown})
	if model.scroll != 4 {
		t.Fatalf("PageDown set offset to %d, want 4", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyCtrlD})
	if model.scroll != 6 {
		t.Fatalf("Ctrl+D set offset to %d, want 6", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyPgUp})
	if model.scroll != 2 {
		t.Fatalf("PageUp set offset to %d, want 2", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyCtrlU})
	if model.scroll != 0 {
		t.Fatalf("Ctrl+U set offset to %d, want 0", model.scroll)
	}

	model, _ = model.handleKey(keyMsg("G"))
	if model.scroll != 16 {
		t.Fatalf("G set offset to %d, want last full viewport at 16", model.scroll)
	}

	model, _ = model.handleKey(keyMsg("g"))
	if model.scroll != 0 {
		t.Fatalf("g set offset to %d, want 0", model.scroll)
	}
}

func TestFileNavigationUsesTabAndResetsDiffScroll(t *testing.T) {
	lines := make([]diff.DiffLine, 20)
	model := Model{
		sessions: []diff.Session{
			{Result: &diff.DiffResult{ContentDiff: true, Lines: lines}},
			{Result: &diff.DiffResult{ContentDiff: true, Lines: lines}},
		},
		Height: 13,
		scroll: 5,
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyTab})
	if model.activeIdx != 1 {
		t.Fatalf("Tab selected file %d, want 1", model.activeIdx)
	}
	if model.scroll != 0 {
		t.Fatalf("Tab retained scroll offset %d, want 0", model.scroll)
	}

	model.scroll = 5
	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyShiftTab})
	if model.activeIdx != 0 {
		t.Fatalf("Shift+Tab selected file %d, want 0", model.activeIdx)
	}
	if model.scroll != 0 {
		t.Fatalf("Shift+Tab retained scroll offset %d, want 0", model.scroll)
	}
}

func keyMsg(key string) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
}
