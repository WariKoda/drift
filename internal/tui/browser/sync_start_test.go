package browser

import (
	"testing"

	"github.com/WariKoda/drift/internal/fs"
	tea "github.com/charmbracelet/bubbletea"
)

func TestSyncRequestSnapshotsSelectionsBeforeCommandRuns(t *testing.T) {
	model := Model{Selection: fs.NewSelectionState(), RemoteSelection: fs.NewSelectionState()}
	model.Selection.Toggle("/project/before.txt")
	model.RemoteSelection.Toggle("/remote/before.txt")

	model, cmd := model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("s")})
	if cmd == nil {
		t.Fatal("sync request did not return a command")
	}
	model, _ = model.Update(tea.KeyMsg{Type: tea.KeyEsc})
	model.Selection.Toggle("/project/after.txt")
	model.RemoteSelection.Toggle("/remote/after.txt")

	request, ok := cmd().(MsgSyncRequested)
	if !ok {
		t.Fatal("sync command did not emit a sync request")
	}
	if request.Selection == model.Selection || request.RemoteSelection == model.RemoteSelection {
		t.Fatal("sync request retained mutable browser selections")
	}
	if request.Selection.Count() != 1 || !request.Selection.IsMarked("/project/before.txt") {
		t.Fatalf("local selection was not captured at sync start: %v", request.Selection.Marked)
	}
	if request.RemoteSelection.Count() != 1 || !request.RemoteSelection.IsMarked("/remote/before.txt") {
		t.Fatalf("remote selection was not captured at sync start: %v", request.RemoteSelection.Marked)
	}

	model.Selection.Clear()
	model.RemoteSelection.Clear()
	if request.Selection.Count() != 1 || request.RemoteSelection.Count() != 1 {
		t.Fatal("later browser edits changed the captured sync request")
	}
}
