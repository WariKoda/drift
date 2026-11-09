package diff

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func labelFixture(texts ...string) []DiffLine {
	lines := make([]DiffLine, len(texts))
	local, remote := 0, 0
	for i, text := range texts {
		kind := LineEqual
		switch {
		case strings.HasPrefix(text, "-"):
			kind, text = LineRemoved, text[1:]
		case strings.HasPrefix(text, "+"):
			kind, text = LineAdded, text[1:]
		}
		lines[i] = DiffLine{Text: text, Kind: kind}
		if kind != LineAdded {
			local++
			lines[i].LocalNum = local
		}
		if kind != LineRemoved {
			remote++
			lines[i].RemoteNum = remote
		}
	}
	return lines
}

func hunkLabels(lines []DiffLine, flip bool, name string, context int) []string {
	rows := Flatten(lines, context, nil, flip)
	LabelHunks(lines, rows, flip, name)
	var labels []string
	for _, row := range rows {
		if row.Kind == DisplayHunkHeader {
			labels = append(labels, row.Label)
		}
	}
	return labels
}

func TestLabelHunksUsesNearestPrecedingHeading(t *testing.T) {
	lines := labelFixture(
		"func first() {", "\tone", "\ttwo", "\tthree", "\tfour", "-\told", "+\tnew", "}", "",
		"$handler = function () {", "  body", "  body", "  body", "  body", "-  gone", "};",
	)
	got := hunkLabels(lines, false, "main.go", 1)
	if strings.Join(got, "|") != "func first() {|$handler = function () {" {
		t.Fatalf("labels = %q", got)
	}
}

func TestLabelHunksIgnoresTheHunkAndTheAfterSide(t *testing.T) {
	lines := labelFixture("package main", "", "+func added() {", "\tx", "\tx", "\tx", "\tx", "\tx", "\tx", "-removed()")
	got := hunkLabels(lines, false, "a.go", 0)
	if got[0] != "package main" || got[1] != "package main" {
		t.Fatalf("download preview labels = %q; the after-only line must not count", got)
	}
	if got := hunkLabels(lines, true, "a.go", 0); got[1] != "func added() {" {
		t.Fatalf("upload preview labels = %q; its before side has the added line", got)
	}
	if got := hunkLabels(labelFixture("-first"), false, "a.go", 3); got[0] != "" {
		t.Fatalf("hunk at the start of the file got label %q", got[0])
	}
}

func TestLabelHunksReadsMarkdownHeadings(t *testing.T) {
	lines := labelFixture("# Title", "intro text", "## Usage", "Run it", "", "####### seven", "#nospace", "text", "-old", "+new")
	if got := hunkLabels(lines, false, "README.MD", 1); got[0] != "## Usage" {
		t.Fatalf("markdown label = %q", got[0])
	}
	if got := hunkLabels(lines, false, "notes.txt", 1); got[0] != "Run it" {
		t.Fatalf("plain text label = %q", got[0])
	}
}

func TestHunkHeaderLabelIsSafeAndYieldsToTheRange(t *testing.T) {
	lines := labelFixture("class A\x1b[2J\tdef", "  x", "-y")
	rows := Flatten(lines, 0, nil, false)
	LabelHunks(lines, rows, false, "a.rb")
	header := rows[1]
	if header.Kind != DisplayHunkHeader || header.Label == "" {
		t.Fatalf("fixture header = %+v", rows)
	}
	for _, width := range []int{1, 10, 22, 30, 80} {
		out := RenderUnifiedRows(&DiffResult{Lines: lines}, rows, width, 1, 1, RenderOptions{})
		plain := out[0]
		if strings.Contains(plain, "\x1b[2J") || lipgloss.Width(plain) != width {
			t.Fatalf("width %d: unsafe or misfit header %q", width, plain)
		}
		if width >= 22 && !strings.Contains(plain, header.Header) {
			t.Fatalf("width %d dropped the range: %q", width, plain)
		}
		if width >= 80 && !strings.Contains(plain, "class A") {
			t.Fatalf("width %d dropped the label: %q", width, plain)
		}
	}
	long := strings.Repeat("é", maxLabelBytes)
	if label := trimLabel(long); len(label) > maxLabelBytes || !strings.HasPrefix(long, label) || !strings.HasSuffix(label, "é") {
		t.Fatalf("trimmed label is not a valid prefix: %q", label)
	}
}
