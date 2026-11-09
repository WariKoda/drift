package diffview

import (
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

func TestSyntaxPreferenceInvalidatesPendingDecoration(t *testing.T) {
	result := &diff.DiffResult{Lines: []diff.DiffLine{{Text: "const n=1", Kind: diff.LineRemoved, LocalNum: 1}, {Text: "const n=2", Kind: diff.LineAdded, RemoteNum: 1}}}
	m := New([]diff.Session{{LocalPath: "a.go", Result: result}}, config.Host{}, nil, nil, 100, 30)
	m.SetPreferences(config.DiffPreferences{Syntax: true})
	cmd := m.Init()
	old := cmd().(MsgDecorated)
	if old.syntax == nil {
		t.Fatal("syntax enabled but no tokens")
	}
	m.SetPreferences(config.DiffPreferences{})
	m.acceptDecoration(old)
	if m.decorationFor(0) != nil {
		t.Fatal("late tokens accepted after disabling syntax")
	}
	cmd = m.Init()
	current := cmd().(MsgDecorated)
	if current.syntax != nil {
		t.Fatal("disabled syntax did work")
	}
	m.acceptDecoration(current)
	if _, ready := m.columnLimit(); !ready || m.decorationFor(0).syntax != nil {
		t.Fatal("plain layout did not retain geometry")
	}
}

func TestDecorationRejectsStaleAndClosedRequests(t *testing.T) {
	result := &diff.DiffResult{Lines: []diff.DiffLine{{Text: "prefix old", Kind: diff.LineRemoved}, {Text: "prefix new", Kind: diff.LineAdded}}}
	m := New([]diff.Session{{Result: result}, {Result: &diff.DiffResult{Lines: result.Lines}}}, config.Host{}, nil, nil, 100, 30)
	cmd := m.Init()
	if cmd == nil {
		t.Fatal("no highlight command")
	}
	first := cmd().(MsgDecorated)
	m.selectFile(1)
	next := m.startDecoration()
	m.acceptDecoration(first)
	if m.decorationFor(0) != nil || m.decorationFor(1) != nil {
		t.Fatal("stale highlight accepted")
	}
	second := next().(MsgDecorated)
	m.acceptDecoration(second)
	if m.decorationFor(1) == nil || len(m.decorationFor(1).spans) == 0 {
		t.Fatal("current highlight missing")
	}
	m.Close()
	m, _ = m.Update(first)
	if m.decorationFor(0) != nil {
		t.Fatal("closed model changed decoration")
	}
}
