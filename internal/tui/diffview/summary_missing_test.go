package diffview

import (
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/charmbracelet/x/ansi"
)

func TestRenderSummaryRowsMissingFile(t *testing.T) {
	modified := time.Date(2026, time.June, 10, 12, 34, 0, 0, time.UTC)
	for _, side := range []string{"local", "remote"} {
		for _, kind := range []string{"empty text", "text", "binary"} {
			t.Run(side+" only/"+kind, func(t *testing.T) {
				r := &diff.DiffResult{Binary: kind == "binary"}
				var size int64
				if kind != "empty text" {
					size = 42
				}
				missing, present := "remote: not present", "local:  2026-06-10 12:34"
				if side == "local" {
					r.LocalOnly, r.ModLocal, r.SizeLocal = true, modified, size
				} else {
					r.RemoteOnly, r.ModRemote, r.SizeRemote = true, modified, size
					missing, present = "local:  not present", "remote: 2026-06-10 12:34"
				}
				rows := (Model{}).renderSummaryRows(r, 5, 100)
				text := ansi.Strip(strings.Join(rows, "\n"))
				for _, want := range []string{side + " only", missing, present} {
					if !strings.Contains(text, want) {
						t.Errorf("summary missing %q: %q", want, text)
					}
				}
				for _, unwanted := range []string{"0001", "identical"} {
					if strings.Contains(text, unwanted) {
						t.Errorf("summary contains %q: %q", unwanted, text)
					}
				}
				if len(rows) != 5 {
					t.Errorf("got %d rows, want 5", len(rows))
				}
			})
		}
	}
}

func TestRenderSummaryRowsBothPresent(t *testing.T) {
	modified := time.Date(2026, time.June, 10, 12, 34, 0, 0, time.UTC)
	for _, binary := range []bool{false, true} {
		r := &diff.DiffResult{Binary: binary, ModLocal: modified, ModRemote: modified, SizeLocal: 42, SizeRemote: 42}
		text := ansi.Strip(strings.Join((Model{}).renderSummaryRows(r, 5, 100), "\n"))
		want := "Files are identical"
		if binary {
			want = "Binary file"
			for _, metadata := range []string{"local:  2026-06-10 12:34  (42 bytes)", "remote: 2026-06-10 12:34  (42 bytes)"} {
				if !strings.Contains(text, metadata) {
					t.Errorf("summary missing %q: %q", metadata, text)
				}
			}
		}
		if !strings.Contains(text, want) || strings.Contains(text, "not present") {
			t.Errorf("unexpected summary: %q", text)
		}
	}
}
