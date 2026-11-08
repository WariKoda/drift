package diffview

import (
	"fmt"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

func stateFixture() *diff.DiffResult {
	lines := make([]diff.DiffLine, 100)
	for i := range lines {
		lines[i] = diff.DiffLine{Text: fmt.Sprintf("source line %d", i), LocalNum: i + 1, RemoteNum: i + 1}
	}
	lines[20].Kind, lines[60].Kind = diff.LineAdded, diff.LineAdded
	return &diff.DiffResult{Lines: lines}
}

func TestViewSnapshotReconcilesContentAndDoesNotRestoreDecisions(t *testing.T) {
	original := stateFixture()
	m := New([]diff.Session{{LocalPath: "file", RemotePath: "/file", Result: original}}, config.Host{}, nil, nil, 120, 20)
	m.syncDirs[0] = DirDeleteLocal
	m.toggleAllFolds()
	m.remapScrollToSource(40)
	snapshot := m.SnapshotView()
	changed := stateFixture()
	changed.Lines = append([]diff.DiffLine{{Text: "new first", LocalNum: 1, RemoteNum: 1}, {Text: "new second", LocalNum: 2, RemoteNum: 2}}, changed.Lines...)
	for i := 2; i < len(changed.Lines); i++ {
		changed.Lines[i].LocalNum += 2
		changed.Lines[i].RemoteNum += 2
	}
	fresh := New([]diff.Session{{LocalPath: "file", RemotePath: "/file", Result: changed}}, config.Host{}, nil, nil, 120, 20)
	decision := fresh.syncDirs[0]
	fresh.RestoreView(snapshot)
	if fresh.topSourceLine() != 42 {
		t.Fatalf("source line = %d, want 42", fresh.topSourceLine())
	}
	if !fresh.gapExpanded(23) {
		t.Fatal("unchanged middle gap was not reconciled")
	}
	if fresh.gapExpanded(0) {
		t.Fatal("changed leading gap was blindly restored")
	}
	if fresh.syncDirs[0] != decision {
		t.Fatal("snapshot restored a transfer decision")
	}
}

func TestVisitedFileKeepsFoldsAndScroll(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "a", Result: stateFixture()}, {LocalPath: "b", Result: stateFixture()}}, config.Host{}, nil, nil, 120, 20)
	m.toggleAllFolds()
	m.remapScrollToSource(40)
	scroll := m.scroll
	m.selectFile(1)
	m.selectFile(0)
	if m.scroll != scroll || !m.gapExpanded(21) {
		t.Fatal("file navigation lost reading state")
	}
}

func TestSnapshotAmbiguousAnchorFallsBack(t *testing.T) {
	result := stateFixture()
	for i := range result.Lines {
		result.Lines[i].Text = "repeated"
	}
	m := New([]diff.Session{{LocalPath: "file", Result: result}}, config.Host{}, nil, nil, 120, 20)
	m.toggleAllFolds()
	m.remapScrollToSource(40)
	fresh := New([]diff.Session{{LocalPath: "file", Result: result}}, config.Host{}, nil, nil, 120, 20)
	fresh.RestoreView(m.SnapshotView())
	got := fresh.scroll
	fresh.scrollToFirstDifference()
	if got != fresh.scroll {
		t.Fatal("ambiguous anchor chose an arbitrary duplicate")
	}
}

func TestSnapshotRemovedFileAndChangedContentFallback(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "a", Result: stateFixture()}}, config.Host{}, nil, nil, 120, 20)
	m.toggleAllFolds()
	m.remapScrollToSource(40)
	snapshot := m.SnapshotView()
	for _, path := range []string{"a", "other"} {
		result := stateFixture()
		for i := range result.Lines {
			result.Lines[i].Text = "replacement"
		}
		fresh := New([]diff.Session{{LocalPath: path, Result: result}}, config.Host{}, nil, nil, 120, 20)
		want := fresh.scroll
		fresh.RestoreView(snapshot)
		if fresh.scroll != want {
			t.Fatal("unmatched content did not fall back to first difference")
		}
	}
}
