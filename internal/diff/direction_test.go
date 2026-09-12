package diff

import (
	"reflect"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

func TestDirectionalPreview(t *testing.T) {
	tests := []struct {
		name, local, remote string
		flip                bool
		header              string
		want                []string
	}{
		{
			name: "download replacement", local: "a\nL1\nL2\nz\n", remote: "a\nR\nz\n",
			header: "@@ -1,4 +1,3 @@",
			want:   []string{"   1    1   a", "   2      - L1", "   3      - L2", "        2 + R", "   4    3   z"},
		},
		{
			name: "upload replacement", local: "a\nL1\nL2\nz\n", remote: "a\nR\nz\n", flip: true,
			header: "@@ -1,3 +1,4 @@",
			want:   []string{"   1    1   a", "   2      - R", "        2 + L1", "        3 + L2", "   3    4   z"},
		},
		{
			name: "download insertion", local: "a\nz\n", remote: "a\nR\nz\n",
			header: "@@ -1,2 +1,3 @@",
			want:   []string{"   1    1   a", "        2 + R", "   2    3   z"},
		},
		{
			name: "upload deletion", local: "a\nz\n", remote: "a\nR\nz\n", flip: true,
			header: "@@ -1,3 +1,2 @@",
			want:   []string{"   1    1   a", "   2      - R", "   3    2   z"},
		},
		{
			name: "download deletion", local: "a\nL\nz\n", remote: "a\nz\n",
			header: "@@ -1,3 +1,2 @@",
			want:   []string{"   1    1   a", "   2      - L", "   3    2   z"},
		},
		{
			name: "upload insertion", local: "a\nL\nz\n", remote: "a\nz\n", flip: true,
			header: "@@ -1,2 +1,3 @@",
			want:   []string{"   1    1   a", "        2 + L", "   2    3   z"},
		},
		{
			name: "download empty target", remote: "R\n",
			header: "@@ -0,0 +1,1 @@", want: []string{"        1 + R"},
		},
		{
			name: "upload empty source", remote: "R\n", flip: true,
			header: "@@ -1,1 +0,0 @@", want: []string{"   1      - R"},
		},
		{
			name: "download empty source", local: "L\n",
			header: "@@ -1,1 +0,0 @@", want: []string{"   1      - L"},
		},
		{
			name: "upload empty target", local: "L\n", flip: true,
			header: "@@ -0,0 +1,1 @@", want: []string{"        1 + L"},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			lines := lineDiff(tt.local, tt.remote)
			original := append([]DiffLine(nil), lines...)
			result := &DiffResult{Lines: lines}
			rows := Flatten(lines, DefaultContext, nil, tt.flip)
			if rows[0].Header != tt.header {
				t.Fatalf("header = %q, want %q", rows[0].Header, tt.header)
			}
			out := RenderUnifiedRows(result, rows, 48, 1, len(rows)-1, tt.flip, true)
			for i := range out {
				if lipgloss.Width(out[i]) != 48 {
					t.Fatalf("row width = %d, want 48", lipgloss.Width(out[i]))
				}
				out[i] = strings.TrimRight(stripANSI(out[i]), " ")
			}
			if !reflect.DeepEqual(out, tt.want) {
				t.Fatalf("rows = %q, want %q", out, tt.want)
			}
			if !reflect.DeepEqual(lines, original) {
				t.Fatal("preview mutated the comparison")
			}
			// Scrolling into a replacement must not change its order or its numbers.
			if len(out) > 2 {
				part := RenderUnifiedRows(result, rows, 48, 2, 2, tt.flip, true)
				for i := range part {
					if got := strings.TrimRight(stripANSI(part[i]), " "); got != tt.want[i+1] {
						t.Fatalf("scrolled row = %q, want %q", got, tt.want[i+1])
					}
				}
			}
		})
	}
}

func TestUploadPreservesFoldAndSourceIdentities(t *testing.T) {
	local := strings.Repeat("before\n", 12) + "L1\nL2\n" + strings.Repeat("after\n", 12)
	remote := strings.Repeat("before\n", 12) + "R\n" + strings.Repeat("after\n", 12)
	lines := lineDiff(local, remote)
	for _, expanded := range []map[int]struct{}{nil, {0: {}, 15: {}}} {
		down := Flatten(lines, DefaultContext, expanded, false)
		up := Flatten(lines, DefaultContext, expanded, true)
		if len(down) != len(up) {
			t.Fatal("direction switch changed display height")
		}
		for i, row := range down {
			if row.Kind == DisplayFold && row != up[i] {
				t.Fatalf("fold identity changed: %+v / %+v", row, up[i])
			}
			if row.Kind == DisplayLine {
				idx := IndexOfSourceLine(up, row.LineIndex)
				if up[idx].Kind != DisplayLine || up[idx].LineIndex != row.LineIndex {
					t.Fatalf("source line %d lost after reordering", row.LineIndex)
				}
			}
		}
	}
}

func TestHunkEmptyRangeUsesPrecedingLine(t *testing.T) {
	lines := lineDiff("a\nz\n", "a\nnew\nz\n")
	if got := formatHunkHeader(lines, 1, 2, false); got != "@@ -1,0 +2,1 @@" {
		t.Fatalf("download insertion header = %q", got)
	}
	if got := formatHunkHeader(lines, 1, 2, true); got != "@@ -2,1 +1,0 @@" {
		t.Fatalf("upload deletion header = %q", got)
	}
}

func TestChangedGuttersUseDiffForegroundAndBackground(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	for _, tt := range []struct {
		kind  LineKind
		flip  bool
		style lipgloss.Style
	}{
		{LineRemoved, false, styles.DiffRemoved},
		{LineAdded, false, styles.DiffAdded},
		{LineRemoved, true, styles.DiffAdded},
		{LineAdded, true, styles.DiffRemoved},
	} {
		line := DiffLine{Kind: tt.kind, Text: "text"}
		if tt.kind == LineRemoved {
			line.LocalNum = 4
		} else {
			line.RemoteNum = 4
		}
		gutter := "   4      "
		if unifiedAction(tt.kind, tt.flip) == actAdd {
			gutter = "        4 "
		}
		row := renderUnifiedLine(line, tt.flip, true, 48, 4, 36)
		if !strings.HasPrefix(row, tt.style.Render(gutter)) {
			t.Fatalf("gutter does not use the change style: %q", row)
		}
		if !strings.Contains(row, tt.style.Render(" "+"text"+strings.Repeat(" ", 32))) {
			t.Fatalf("content background is interrupted: %q", row)
		}
	}
}
