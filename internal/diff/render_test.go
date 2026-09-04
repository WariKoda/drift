package diff

import (
	"strings"
	"testing"
)

func kinds(lines []DiffLine) []LineKind {
	out := make([]LineKind, len(lines))
	for i, l := range lines {
		out[i] = l.Kind
	}
	return out
}

func TestLineDiffEmitsRemovedThenAdded(t *testing.T) {
	lines := lineDiff("a\nB\nc\n", "a\nX\nc\n")
	got := kinds(lines)
	want := []LineKind{LineEqual, LineRemoved, LineAdded, LineEqual}
	if len(got) != len(want) {
		t.Fatalf("got %d lines %v, want %v", len(got), got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("line %d kind = %v, want %v (all: %v)", i, got[i], want[i], got)
		}
	}
	rem := lines[1]
	if rem.Text != "B" {
		t.Fatalf("removed text = %q, want B", rem.Text)
	}
	if rem.LocalNum != 2 || rem.RemoteNum != 0 {
		t.Fatalf("removed line nums = %d/%d, want 2/0", rem.LocalNum, rem.RemoteNum)
	}
	add := lines[2]
	if add.Text != "X" {
		t.Fatalf("added text = %q, want X", add.Text)
	}
	if add.LocalNum != 0 || add.RemoteNum != 2 {
		t.Fatalf("added line nums = %d/%d, want 0/2", add.LocalNum, add.RemoteNum)
	}
}

func TestLineDiffPureAddRemove(t *testing.T) {
	add := lineDiff("a\n", "a\nb\n")
	if k := kinds(add); len(k) != 2 || k[1] != LineAdded {
		t.Fatalf("add kinds = %v, want [Equal Added]", k)
	}
	rem := lineDiff("a\nb\n", "a\n")
	if k := kinds(rem); len(k) != 2 || k[1] != LineRemoved {
		t.Fatalf("remove kinds = %v, want [Equal Removed]", k)
	}
}

func TestUnifiedActionDirection(t *testing.T) {
	tests := []struct {
		kind LineKind
		flip bool
		want lineAct
	}{
		{LineRemoved, false, actRemove},
		{LineAdded, false, actAdd},
		{LineRemoved, true, actAdd},
		{LineAdded, true, actRemove},
		{LineEqual, false, actEqual},
		{LineEqual, true, actEqual},
	}
	for _, tt := range tests {
		if got := unifiedAction(tt.kind, tt.flip); got != tt.want {
			t.Errorf("unifiedAction(%v, flip=%v) = %v, want %v",
				tt.kind, tt.flip, got, tt.want)
		}
	}
}

func TestRenderUnifiedSingleColumn(t *testing.T) {
	result := &DiffResult{Lines: []DiffLine{
		{Text: "same", Kind: LineEqual, LocalNum: 1, RemoteNum: 1},
		{Text: "old", Kind: LineRemoved, LocalNum: 2},
		{Text: "new", Kind: LineAdded, RemoteNum: 2},
	}}
	rows := RenderUnified(result, 40, 0, 3, false)
	if len(rows) != 3 {
		t.Fatalf("got %d rows, want 3", len(rows))
	}
	joined := strings.Join(rows, "\n")
	for _, want := range []string{"same", "old", "new"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("unified output missing %q:\n%s", want, joined)
		}
	}
	// A single-column render must not put old and new on the same padded row.
	if strings.Contains(rows[1], "new") || strings.Contains(rows[2], "old") {
		t.Fatalf("expected separate rows for old/new, got:\n%s", joined)
	}
}

func TestRenderUnifiedDualLineNumbers(t *testing.T) {
	result := &DiffResult{Lines: []DiffLine{
		{Text: "same", Kind: LineEqual, LocalNum: 1, RemoteNum: 1},
		{Text: "old", Kind: LineRemoved, LocalNum: 2},
		{Text: "new", Kind: LineAdded, RemoteNum: 3},
	}}
	rows := RenderUnified(result, 48, 0, 3, false)
	// Strip ANSI so we can assert on the plain gutter layout.
	plain := make([]string, len(rows))
	for i, row := range rows {
		plain[i] = stripANSI(row)
	}
	if !strings.HasPrefix(strings.TrimRight(plain[0], " "), "   1    1   same") {
		t.Fatalf("equal row gutter = %q, want dual nums 1/1", plain[0])
	}
	if !strings.Contains(plain[1], "   2") || !strings.Contains(plain[1], "- old") {
		t.Fatalf("removed row = %q, want local num 2 and - marker", plain[1])
	}
	if !strings.Contains(plain[2], "   3") || !strings.Contains(plain[2], "+ new") {
		t.Fatalf("added row = %q, want remote num 3 and + marker", plain[2])
	}
}

func TestRenderUnifiedRowsHeaderAndFold(t *testing.T) {
	result := &DiffResult{Lines: []DiffLine{
		{Text: "same", Kind: LineEqual, LocalNum: 1, RemoteNum: 1},
		{Text: "old", Kind: LineRemoved, LocalNum: 2},
	}}
	rows := []DisplayRow{
		{Kind: DisplayHunkHeader, Header: "@@ -1,2 +1,1 @@"},
		{Kind: DisplayLine, LineIndex: 0},
		{Kind: DisplayFold, Hidden: 42},
	}
	out := RenderUnifiedRows(result, rows, 48, 0, 3, false)
	if len(out) != 3 {
		t.Fatalf("got %d rows", len(out))
	}
	h := stripANSI(out[0])
	if !strings.Contains(h, "@@ -1,2 +1,1 @@") || !strings.Contains(h, "┄") {
		t.Fatalf("header row = %q", h)
	}
	if !strings.Contains(stripANSI(out[1]), "same") {
		t.Fatalf("line row = %q", stripANSI(out[1]))
	}
	f := stripANSI(out[2])
	if !strings.Contains(f, "▸") || !strings.Contains(f, "42 unchanged lines") {
		t.Fatalf("fold row = %q", f)
	}
}

func stripANSI(s string) string {
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		if s[i] == '\x1b' {
			for i < len(s) && s[i] != 'm' {
				i++
			}
			continue
		}
		b.WriteByte(s[i])
	}
	return b.String()
}

func TestRenderUnifiedLineDropsCarriageReturn(t *testing.T) {
	row := renderUnifiedLine(DiffLine{Text: "}\r", Kind: LineAdded, RemoteNum: 25}, false, 80, 4, 68)
	if strings.Contains(row, "\r") {
		t.Fatal("rendered add line still contains CR")
	}
	equal := renderUnifiedLine(DiffLine{Text: "same\r", Kind: LineEqual, LocalNum: 1, RemoteNum: 1}, false, 80, 4, 68)
	if strings.Contains(equal, "\r") {
		t.Fatal("rendered equal line still contains CR")
	}
}
