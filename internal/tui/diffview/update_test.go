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

func keyMsg(key string) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
}
