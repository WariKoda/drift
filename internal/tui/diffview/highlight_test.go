package diffview

import (
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

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
	if len(m.spans) != 0 {
		t.Fatal("stale highlight accepted")
	}
	second := next().(MsgDecorated)
	m.acceptDecoration(second)
	if len(m.spans) == 0 {
		t.Fatal("current highlight missing")
	}
	m.Close()
	before := m.spans
	m, _ = m.Update(first)
	if len(m.spans) != len(before) {
		t.Fatal("closed model changed decoration")
	}
}
