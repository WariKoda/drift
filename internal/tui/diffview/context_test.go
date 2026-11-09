package diffview

import (
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

// contextAround counts the equal rows shown directly above the second change
// of stateFixture, at line index 60 after a 39-line equal run.
func contextAround(m Model) int {
	rows := m.displayRows()
	at := diff.IndexOfSourceLine(rows, 60)
	count := 0
	for i := at - 1; i >= 0 && rows[i].Kind == diff.DisplayLine; i-- {
		count++
	}
	return count
}

func foldOf(rows []diff.DisplayRow, gapID int) (diff.DisplayRow, int, bool) {
	for i, row := range rows {
		if row.Kind == diff.DisplayFold && row.GapID == gapID {
			return row, i, true
		}
	}
	return diff.DisplayRow{}, 0, false
}

func hiddenTotal(rows []diff.DisplayRow) int {
	total := 0
	for _, row := range rows {
		if row.Kind == diff.DisplayFold {
			total += row.Hidden
		}
	}
	return total
}

func TestContextKeysChangeEveryFileWithinBounds(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}, {LocalPath: "/p/b", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 20)
	if contextAround(m) != diff.DefaultContext {
		t.Fatalf("default context %d", contextAround(m))
	}
	for _, step := range []struct {
		key  string
		want int
	}{{"+", 5}, {"=", 10}, {"+", 25}, {"+", 25}, {"-", 10}, {"-", 5}, {"-", 3}, {"-", 1}, {"-", 0}, {"-", 0}} {
		source := m.topSourceLine()
		m, _ = m.handleKey(keyMsg(step.key))
		if got := contextAround(m); got != step.want {
			t.Fatalf("after %q context %d, want %d", step.key, got, step.want)
		}
		if at := diff.IndexOfSourceLine(m.displayRows(), source); at < m.scroll || at >= m.scroll+m.viewportHeight() {
			t.Fatalf("after %q line %d left the viewport", step.key, source)
		}
	}
	m.selectFile(1)
	if contextAround(m) != 0 {
		t.Fatal("context is not shared by all files")
	}
}

func TestRevealAddsLinesUntilTheFoldDisappears(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 14)
	before := hiddenTotal(m.displayRows())
	fold, at, _ := foldOf(m.displayRows(), 21)
	m.scroll = at
	m, _ = m.handleKey(keyMsg("L"))
	if got, _, ok := foldOf(m.displayRows(), 21); !ok || got.Hidden != fold.Hidden-2*revealStep {
		t.Fatalf("inner fold hides %d lines after L, want %d", got.Hidden, fold.Hidden-2*revealStep)
	}
	if m.topSourceLine() != fold.FoldStart {
		t.Fatalf("top moved to line %d, want the first revealed line %d", m.topSourceLine(), fold.FoldStart)
	}
	_, m.scroll, _ = foldOf(m.displayRows(), 21)
	m, _ = m.handleKey(keyMsg("L"))
	if _, _, ok := foldOf(m.displayRows(), 21); ok {
		t.Fatal("a reveal covering the run kept its fold")
	}
	m, _ = m.handleKey(keyMsg("c"))
	if len(m.revealed[0]) != 0 || hiddenTotal(m.displayRows()) != before {
		t.Fatal("c did not reset revealed context")
	}
	m, _ = m.handleKey(keyMsg("L"))
	m.scroll = 0
	m, _ = m.handleKey(keyMsg("h"))
	if len(m.revealed[0]) != 0 {
		t.Fatal("h did not reset the revealed context in view")
	}
}

func TestRevealInContinuousViewTargetsTheVisibleFile(t *testing.T) {
	m := continuousModel(t, 100, 14)
	m.selectSection(4)
	m, _ = m.handleKey(keyMsg("L"))
	if m.revealed[4][0] != revealStep || len(m.revealed[0]) != 0 {
		t.Fatalf("revealed %v, want the first fold of file 4", m.revealed)
	}
	if row := topRow(m); row.session != 4 || row.kind != streamTitle {
		t.Fatalf("top row moved to %+v", row)
	}
}

func TestContextAndRevealSurviveRefresh(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 40)
	m, _ = m.handleKey(keyMsg("+"))
	m.scroll = 0
	m, _ = m.handleKey(keyMsg("L"))
	want := m.displayRows()
	restored := New([]diff.Session{{LocalPath: "/p/a", Result: stateFixture()}}, config.Host{}, nil, nil, 100, 40)
	restored.RestoreView(m.SnapshotView())
	if restored.context() != 5 || len(restored.displayRows()) != len(want) || hiddenTotal(restored.displayRows()) != hiddenTotal(want) {
		t.Fatalf("restored context %d with %d rows, want 5 and %d", restored.context(), len(restored.displayRows()), len(want))
	}
}
