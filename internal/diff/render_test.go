package diff

import "testing"

func kinds(lines []DiffLine) []LineKind {
	out := make([]LineKind, len(lines))
	for i, l := range lines {
		out[i] = l.Kind
	}
	return out
}

func TestLineDiffPairsModification(t *testing.T) {
	lines := lineDiff("a\nB\nc\n", "a\nX\nc\n")
	got := kinds(lines)
	want := []LineKind{LineEqual, LineModified, LineEqual}
	if len(got) != len(want) {
		t.Fatalf("got %d lines %v, want %v", len(got), got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("line %d kind = %v, want %v (all: %v)", i, got[i], want[i], got)
		}
	}
	mod := lines[1]
	if mod.LocalLine != "B" || mod.RemoteLine != "X" {
		t.Fatalf("modified line = %q/%q, want B/X", mod.LocalLine, mod.RemoteLine)
	}
	if mod.LocalNum != 2 || mod.RemoteNum != 2 {
		t.Fatalf("modified line nums = %d/%d, want 2/2", mod.LocalNum, mod.RemoteNum)
	}
}

func TestLineDiffPureAddRemove(t *testing.T) {
	// remote has an extra line -> pure addition; local has an extra -> removal
	add := lineDiff("a\n", "a\nb\n")
	if k := kinds(add); len(k) != 2 || k[1] != LineAdded {
		t.Fatalf("add kinds = %v, want [Equal Added]", k)
	}
	rem := lineDiff("a\nb\n", "a\n")
	if k := kinds(rem); len(k) != 2 || k[1] != LineRemoved {
		t.Fatalf("remove kinds = %v, want [Equal Removed]", k)
	}
}

func TestSideActionDirection(t *testing.T) {
	tests := []struct {
		kind          LineKind
		isLocal, flip bool
		want          sideAct
	}{
		// download / neutral (flip=false): remote is "new"
		{LineRemoved, true, false, actRemove},
		{LineRemoved, false, false, actBlank},
		{LineAdded, false, false, actAdd},
		{LineAdded, true, false, actBlank},
		{LineModified, true, false, actRemove}, // local=old
		{LineModified, false, false, actAdd},   // remote=new
		// upload (flip=true): local is "new"
		{LineRemoved, true, true, actAdd},      // local-only -> will be added remotely
		{LineAdded, false, true, actRemove},    // remote-only -> will be removed
		{LineModified, true, true, actAdd},     // local=new
		{LineModified, false, true, actRemove}, // remote=old
		{LineEqual, true, false, actEqual},
	}
	for _, tt := range tests {
		if got := sideAction(tt.kind, tt.isLocal, tt.flip); got != tt.want {
			t.Errorf("sideAction(%v, local=%v, flip=%v) = %v, want %v",
				tt.kind, tt.isLocal, tt.flip, got, tt.want)
		}
	}
}
