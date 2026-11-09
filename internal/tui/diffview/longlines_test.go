package diffview

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

func longFixture() *diff.DiffResult {
	return &diff.DiffResult{Lines: []diff.DiffLine{
		{Text: "header", LocalNum: 1, RemoteNum: 1},
		{Text: strings.Repeat("a", longLineBytes), Kind: diff.LineRemoved, LocalNum: 2},
		{Text: strings.Repeat("b", longLineBytes), Kind: diff.LineAdded, RemoteNum: 2},
	}}
}

func TestLongLineFileOpensAsSummary(t *testing.T) {
	m := New([]diff.Session{{LocalPath: "/p/bundle.js", Result: longFixture()}}, config.Host{}, nil, nil, 100, 14)
	decision := m.syncDirs[0]
	if m.Init() != nil {
		t.Fatal("hidden long-line diff started presentation work")
	}
	plain := stripANSI(m.View())
	if len(m.displayRows()) != 0 || !strings.Contains(plain, "4096 bytes long") || strings.Contains(plain, "aaaa") {
		t.Fatalf("long-line file was not summarized: %s", plain)
	}
	if !strings.Contains(plain, "+1 -1") {
		t.Fatal("summary dropped the line counts")
	}
	m, _ = m.handleKey(keyMsg("o"))
	if plain := stripANSI(m.View()); len(m.displayRows()) == 0 || !strings.Contains(plain, "aaaa") && !strings.Contains(plain, "bbbb") {
		t.Fatal("o did not show the diff")
	}
	m, _ = m.handleKey(keyMsg("o"))
	if len(m.displayRows()) != 0 || m.syncDirs[0] != decision {
		t.Fatal("second o did not hide the diff or changed the sync decision")
	}
}

func TestLongLineThresholdIgnoresUnchangedLines(t *testing.T) {
	result := longFixture()
	result.Lines[1].Kind, result.Lines[1].RemoteNum = diff.LineEqual, 2
	result.Lines[2] = diff.DiffLine{Text: "short", Kind: diff.LineAdded, RemoteNum: 3}
	m := New([]diff.Session{{LocalPath: "/p/a", Result: result}}, config.Host{}, nil, nil, 100, 14)
	if m.longHidden(0) {
		t.Fatal("a long unchanged line hid the diff")
	}
	m, _ = m.handleKey(keyMsg("o"))
	if m.longOpen[0] {
		t.Fatal("o toggled a file without long changed lines")
	}
}

func TestLongLineFilesInContinuousViewAndNavigation(t *testing.T) {
	sessions := []diff.Session{
		{LocalPath: "/p/a", Result: stateFixture()},
		{LocalPath: "/p/bundle.js", Result: longFixture()},
		{LocalPath: "/p/c", Result: stateFixture()},
	}
	m := New(sessions, config.Host{}, nil, nil, 100, 14)
	for m.activeIdx == 0 {
		m, _ = m.handleKey(keyMsg("]"))
	}
	if m.activeIdx != 2 {
		t.Fatalf("] stopped in file %d, want the long-line file skipped", m.activeIdx)
	}
	m.SetPreferences(config.DiffPreferences{Continuous: true})
	if r := m.stream.rows[m.stream.start[1]+1]; r.kind != streamNote {
		t.Fatalf("long-line section shows %+v, want its summary", r)
	}
	m.selectSection(1)
	m, _ = m.handleKey(keyMsg("o"))
	if r := m.stream.rows[m.stream.start[1]+1]; r.kind != streamLine {
		t.Fatal("o did not open the section")
	}
	restored := New(sessions, config.Host{}, nil, nil, 100, 14)
	restored.RestoreView(m.SnapshotView())
	if !restored.longOpen[1] || restored.longHidden(1) {
		t.Fatal("refresh hid the opened long-line diff again")
	}
}
