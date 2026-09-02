package diffview

import (
	"fmt"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
)

func TestViewRendersSideBySideFileListAndDiff(t *testing.T) {
	lines := make([]diff.DiffLine, 8)
	for i := range lines {
		lineNumber := i + 1
		lines[i] = diff.DiffLine{
			Text:      fmt.Sprintf("row-%02d", lineNumber),
			LocalNum:  lineNumber,
			RemoteNum: lineNumber,
			Kind:      diff.LineEqual,
		}
	}
	model := Model{
		sessions: []diff.Session{{
			LocalPath:  "/local/file.txt",
			RemotePath: "/remote/file.txt",
			Result:     &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		syncDirs: []SyncDir{DirNone},
		scroll:   2,
		Width:    100,
		Height:   12,
	}

	rendered := strings.Split(model.View(), "\n")
	if len(rendered) != model.Height {
		t.Fatalf("view rendered %d rows, want terminal height %d", len(rendered), model.Height)
	}

	fw := model.fileListWidth()
	contentStart := model.contentTop()
	for i, row := range rendered[contentStart : contentStart+model.viewportHeight()] {
		lineNumber := i + 3
		text := fmt.Sprintf("row-%02d", lineNumber)
		if !strings.Contains(row, text) {
			t.Fatalf("viewport row %d does not contain unified line %d: %q", i, lineNumber, row)
		}
		// Diff content sits to the right of the file-list column.
		plain := stripANSI(row)
		idx := strings.Index(plain, text)
		if idx < fw {
			t.Fatalf("diff text %q appeared in the file list (col %d < %d): %q", text, idx, fw, plain)
		}
	}

	// File list row shows the short local path on the left.
	fileRow := stripANSI(rendered[bodyTop])
	if !strings.Contains(fileRow, "file.txt") {
		t.Fatalf("file list row missing name: %q", fileRow)
	}
}

func TestViewRendersBulkSyncFailureContext(t *testing.T) {
	model := Model{
		syncErrors: []SyncFailure{{
			Operation: "upload",
			Path:      "/project/file.php",
			Reason:    "permission denied",
		}},
		showErrors: true,
		Width:      100,
		Height:     12,
	}

	rendered := model.View()
	for _, want := range []string{"file.php", "upload", "permission denied"} {
		if !strings.Contains(rendered, want) {
			t.Fatalf("error details do not contain %q: %q", want, rendered)
		}
	}
}

func TestSyncProgressLabel(t *testing.T) {
	tests := []struct {
		name string
		done int
		tot  int
		want string
	}{
		{"no total falls back", 0, 0, "syncing…"},
		{"start", 0, 10, "syncing [░░░░░░░░░░] 0/10"},
		{"partway", 4, 10, "syncing [████░░░░░░] 4/10"},
		{"complete", 10, 10, "syncing [██████████] 10/10"},
		{"single file done", 1, 1, "syncing [██████████] 1/1"},
		{"overshoot clamped", 12, 10, "syncing [██████████] 12/10"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := Model{syncDone: tt.done, syncTotal: tt.tot}
			if got := m.syncProgressLabel(); got != tt.want {
				t.Fatalf("syncProgressLabel() = %q, want %q", got, tt.want)
			}
		})
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
