package diffview

import (
	"fmt"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
)

func TestViewRendersSynchronizedDiffViewport(t *testing.T) {
	lines := make([]diff.DiffLine, 8)
	for i := range lines {
		lineNumber := i + 1
		lines[i] = diff.DiffLine{
			LocalLine:  fmt.Sprintf("local-row-%02d", lineNumber),
			RemoteLine: fmt.Sprintf("remote-row-%02d", lineNumber),
			LocalNum:   lineNumber,
			RemoteNum:  lineNumber,
			Kind:       diff.LineEqual,
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
		Height:   12, // one file row and four diff rows
	}

	rendered := strings.Split(model.View(), "\n")
	if len(rendered) != model.Height {
		t.Fatalf("view rendered %d rows, want terminal height %d", len(rendered), model.Height)
	}

	const contentStart = 6
	for i, row := range rendered[contentStart : contentStart+model.viewportHeight()] {
		lineNumber := i + 3
		local := fmt.Sprintf("local-row-%02d", lineNumber)
		remote := fmt.Sprintf("remote-row-%02d", lineNumber)
		if !strings.Contains(row, local) || !strings.Contains(row, remote) {
			t.Fatalf("viewport row %d does not contain synchronized line %d: %q", i, lineNumber, row)
		}
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
