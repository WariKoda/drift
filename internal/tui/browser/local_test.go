package browser

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestLocalExpansionDefersFilesystemIOAndReportsErrors(t *testing.T) {
	m := localExpansionTestModel(t, 1)
	path := m.entries[m.cursor].Path
	m, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRight})
	if cmd == nil || len(m.entries) != 5 || m.localReads[path] == nil {
		t.Fatal("expand did not schedule a read independently of the visible tree")
	}
	if err := os.RemoveAll(path); err != nil {
		t.Fatal(err)
	}
	result := cmd().(MsgLocalLoaded)
	if result.err == nil {
		t.Fatal("directory was read before the command executed")
	}
	m, _ = m.Update(result)
	if m.entries[m.cursor].Expanded || len(m.entries) != 5 || !strings.Contains(m.statusMsg, "Local read failed") {
		t.Fatal("failed expansion changed the tree or swallowed its error")
	}
}

func TestLocalCollapseAndNewReadRejectLateResults(t *testing.T) {
	m := localExpansionTestModel(t, 1)
	first := m.expandAt(4)().(MsgLocalLoaded)
	m.collapseAt(4)
	writeVisibilityFile(t, filepath.Join(m.WorkDir, "target", "new.txt"), "new")
	second := m.expandAt(4)().(MsgLocalLoaded)
	m, _ = m.Update(second)
	assertVisibleNames(t, m.entries[5:], "a.txt", "new.txt")
	m, _ = m.Update(first)
	assertVisibleNames(t, m.entries[5:], "a.txt", "new.txt")

	other, err := New(m.WorkDir)
	if err != nil {
		t.Fatal(err)
	}
	other, _ = other.Update(second)
	if len(other.entries) != 5 {
		t.Fatal("a result from an old browser identity entered the new tree")
	}
}

func TestLocalRefreshIsAtomicLatestWinsAndCancellable(t *testing.T) {
	m := localExpansionTestModel(t, 1)
	firstCmd := m.RefreshLocal()
	first := firstCmd().(MsgLocalLoaded)
	writeVisibilityFile(t, filepath.Join(m.WorkDir, "latest.txt"), "new")
	secondCmd := m.RefreshLocal()
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeySpace})
	selected := m.filteredEntries()[m.cursor].Path
	if !m.Selection.IsMarked(selected) {
		t.Fatal("pending refresh blocked normal selection")
	}
	m, _ = m.Update(secondCmd())
	if len(m.entries) != 6 || m.filteredEntries()[m.cursor].Path != selected || !m.Selection.IsMarked(selected) {
		t.Fatal("refresh lost live navigation or selection")
	}
	m, _ = m.Update(first)
	if len(m.entries) != 6 {
		t.Fatal("an older refresh replaced the new tree")
	}
	canceled := m.RefreshLocal()
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if m.localReload != nil || !m.Selection.IsMarked(selected) {
		t.Fatal("cancel did not invalidate the read or unexpectedly cleared marks")
	}
	m, _ = m.Update(canceled())
	if len(m.entries) != 6 || m.statusMsg != "Local read cancelled" {
		t.Fatal("a cancelled refresh changed the browser")
	}
}

func TestLocalRefreshPreservesExpandedHierarchyAndUpdatesIgnoreSnapshot(t *testing.T) {
	root := t.TempDir()
	writeVisibilityFile(t, filepath.Join(root, "dir", "nested", "child.txt"), "child")
	writeVisibilityFile(t, filepath.Join(root, "dir", "nested", "ignored.txt"), "ignored")
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	m, _ = m.Update(m.expandAt(0)())
	m, _ = m.Update(m.expandAt(1)())
	m.cursor = 2
	current := m.entries[m.cursor].Path
	m.Selection.Toggle(current)
	writeVisibilityFile(t, filepath.Join(root, ".gitignore"), "ignored.txt\n")
	writeVisibilityFile(t, filepath.Join(root, "dir", "new.txt"), "new")
	m, _ = m.Update(m.RefreshLocal()())
	assertVisibleNames(t, m.filteredEntries(), "dir", "nested", "child.txt", "new.txt")
	if m.filteredEntries()[m.cursor].Path != current || !m.Selection.IsMarked(current) {
		t.Fatal("refresh did not preserve the current path and mark")
	}
	if m.entries[2].Depth != 2 || m.entries[2].Parent != m.entries[1] || m.entries[1].Parent != m.entries[0] {
		t.Fatal("refresh did not restore valid tree parent/depth links")
	}
	late := m.RefreshLocal()()
	m.collapseAt(0)
	m, _ = m.Update(late)
	assertVisibleNames(t, m.filteredEntries(), "dir")
}

func TestLocalRefreshFailureKeepsPreviousTreeAndClassifier(t *testing.T) {
	root := filepath.Join(t.TempDir(), "project")
	writeVisibilityFile(t, filepath.Join(root, "file.txt"), "file")
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	classifier, entry := m.classifier, m.entries[0]
	cmd := m.RefreshLocal()
	if err := os.RemoveAll(root); err != nil {
		t.Fatal(err)
	}
	m, _ = m.Update(cmd())
	if m.classifier != classifier || len(m.entries) != 1 || m.entries[0] != entry || !strings.Contains(m.statusMsg, "Local read failed") {
		t.Fatal("refresh failure replaced the previous snapshot or hid the error")
	}
}

func TestLocalCommandDoesNotMutateLiveTreeConcurrently(t *testing.T) {
	m := localExpansionTestModel(t, 100)
	cmd := m.expandAt(4)
	result := make(chan tea.Msg, 1)
	go func() { result <- cmd() }()
	for i := 0; i < 50; i++ {
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyUp})
		_ = m.View()
	}
	m, _ = m.Update(<-result)
	if len(m.entries) != 105 || m.entries[m.cursor].Name != "a" {
		t.Fatal("background expansion moved the live cursor or lost its children")
	}
}
