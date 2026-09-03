package diffview

import (
	"errors"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	tea "github.com/charmbracelet/bubbletea"
)

func TestQuickSyncBlocksOtherRemoteActions(t *testing.T) {
	model := Model{
		sessions: []diff.Session{{
			LocalPath:  "/local/file.txt",
			RemotePath: "/remote/file.txt",
			Result:     &diff.DiffResult{ContentDiff: true},
		}},
		syncDirs: []SyncDir{DirUpload},
	}

	model, cmd := model.handleKey(keyMsg("u"))
	if cmd == nil {
		t.Fatal("quick upload did not return a command")
	}
	if !model.quickSyncing {
		t.Fatal("quick upload did not mark the remote connection busy")
	}

	for _, key := range []string{"u", "d", "r", "s", "S", "q", "esc"} {
		t.Run(key, func(t *testing.T) {
			next, cmd := model.handleKey(keyMsg(key))
			if cmd != nil {
				t.Fatalf("key %q started another remote command during quick sync", key)
			}
			if !next.quickSyncing {
				t.Fatalf("key %q cleared the quick-sync state", key)
			}
		})
	}
}

func TestQuickSyncContinuesThroughAsyncDiffRefresh(t *testing.T) {
	model := Model{
		sessions: []diff.Session{{
			LocalPath:  "/local/file.txt",
			RemotePath: "/remote/file.txt",
			Result:     &diff.DiffResult{ContentDiff: true},
		}},
		syncDirs: []SyncDir{DirUpload},
	}

	model, _ = model.handleKey(keyMsg("u"))
	model, cmd := model.Update(MsgSynced{SessionIdx: 0, Direction: DirUpload})
	if cmd == nil {
		t.Fatal("successful quick sync did not schedule an asynchronous diff refresh")
	}
	label, tracker, active := model.LoadingActivity()
	if !active || tracker == nil || label != "Refreshing diff…" {
		t.Fatalf("activity = (%q, %v, %v), want active diff refresh", label, tracker, active)
	}
}

func TestQuickSyncErrorReleasesRemoteActions(t *testing.T) {
	model := Model{
		quickSyncing: true,
		sessions:     []diff.Session{{Result: &diff.DiffResult{ContentDiff: true}}},
		syncDirs:     []SyncDir{DirUpload},
	}

	model, _ = model.Update(MsgSyncError{Err: errors.New("transfer failed")})
	if model.quickSyncing {
		t.Fatal("quick-sync state remained active after an error")
	}
	if model.sessions[0].Err == nil {
		t.Fatal("quick-sync error was not recorded on the active session")
	}
}

func TestBulkSyncFailureOpensDetails(t *testing.T) {
	tracker := NewLoadProgressTracker()
	model := Model{
		syncing:         true,
		activityTracker: tracker,
		sessions:        []diff.Session{{LocalPath: "/project/file.php", RemotePath: "/srv/file.php"}},
		syncDirs:        []SyncDir{DirUpload},
	}
	failure := SyncFailure{
		Operation: "upload",
		Path:      "/project/file.php",
		Reason:    "permission denied",
	}

	model, _ = model.Update(MsgBulkSyncDone{Errors: []SyncFailure{failure}})

	if !model.showErrors {
		t.Fatal("bulk-sync failure details were not opened")
	}
	if len(model.syncErrors) != 1 || model.syncErrors[0] != failure {
		t.Fatalf("sync errors = %#v, want %#v", model.syncErrors, failure)
	}
}

func TestBulkSyncAndRefreshBlockQuickSync(t *testing.T) {
	base := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true},
		}},
		syncDirs: []SyncDir{DirUpload},
	}

	for _, test := range []struct {
		name  string
		model Model
	}{
		{name: "bulk sync", model: func() Model {
			model := base
			model.syncing = true
			return model
		}()},
		{name: "refresh", model: func() Model {
			model := base
			model.refreshing = true
			return model
		}()},
	} {
		t.Run(test.name, func(t *testing.T) {
			next, cmd := test.model.handleKey(keyMsg("u"))
			if cmd != nil {
				t.Fatal("quick upload started while another remote action was active")
			}
			if next.quickSyncing {
				t.Fatal("blocked quick upload marked itself active")
			}
		})
	}
}

func TestDiffScrollNavigation(t *testing.T) {
	lines := make([]diff.DiffLine, 20)
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 12, // body leaves a six-line diff viewport beside the file list
	}

	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyRunes, Runes: []rune("j")},
		{Type: tea.KeyDown},
		{Type: tea.KeyRunes, Runes: []rune("J")},
	} {
		model, _ = model.handleKey(key)
	}
	if model.scroll != 3 {
		t.Fatalf("line scrolling set offset to %d, want 3", model.scroll)
	}

	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyRunes, Runes: []rune("k")},
		{Type: tea.KeyUp},
		{Type: tea.KeyRunes, Runes: []rune("K")},
		{Type: tea.KeyUp},
	} {
		model, _ = model.handleKey(key)
	}
	if model.scroll != 0 {
		t.Fatalf("upward scrolling set offset to %d, want clamped offset 0", model.scroll)
	}
}

func TestDiffPageAndBoundaryNavigation(t *testing.T) {
	lines := make([]diff.DiffLine, 20)
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 12, // body leaves a six-line diff viewport beside the file list
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyPgDown})
	if model.scroll != 6 {
		t.Fatalf("PageDown set offset to %d, want 6", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyCtrlD})
	if model.scroll != 9 {
		t.Fatalf("Ctrl+D set offset to %d, want 9", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyPgUp})
	if model.scroll != 3 {
		t.Fatalf("PageUp set offset to %d, want 3", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyCtrlU})
	if model.scroll != 0 {
		t.Fatalf("Ctrl+U set offset to %d, want 0", model.scroll)
	}

	model, _ = model.handleKey(keyMsg("G"))
	if model.scroll != 14 {
		t.Fatalf("G set offset to %d, want last full viewport at 14", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyHome})
	if model.scroll != 0 {
		t.Fatalf("Home set offset to %d, want 0", model.scroll)
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyEnd})
	if model.scroll != 14 {
		t.Fatalf("End set offset to %d, want last full viewport at 14", model.scroll)
	}

	model, _ = model.handleKey(keyMsg("g"))
	if model.scroll != 0 {
		t.Fatalf("g set offset to %d, want 0", model.scroll)
	}
}

func TestHunkNavigationClampsToLastFullViewport(t *testing.T) {
	lines := make([]diff.DiffLine, 20)
	lines[len(lines)-1].Kind = diff.LineRemoved
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 12,
	}

	model, _ = model.handleKey(keyMsg("]"))
	want := clampedHeader(lines, 12)
	if model.scroll != want {
		t.Fatalf("hunk navigation set offset to %d, want last full viewport at %d", model.scroll, want)
	}
}

func TestShortDiffDoesNotScroll(t *testing.T) {
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: make([]diff.DiffLine, 2)},
		}},
		Height: 12,
	}

	for _, key := range []tea.KeyMsg{
		{Type: tea.KeyDown},
		{Type: tea.KeyPgDown},
		{Type: tea.KeyEnd},
	} {
		model, _ = model.handleKey(key)
	}
	if model.scroll != 0 {
		t.Fatalf("short diff scrolled to offset %d, want 0", model.scroll)
	}
}

func TestResizeClampsDiffScrollToNewViewport(t *testing.T) {
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: make([]diff.DiffLine, 20)},
		}},
		Height: 12,
		scroll: 16,
	}

	model.SetSize(100, 16) // viewport grows from six to ten lines
	if model.scroll != 10 {
		t.Fatalf("resize retained offset %d, want clamped offset 10", model.scroll)
	}
}

func TestNewScrollsToFirstTextualDifference(t *testing.T) {
	tests := []struct {
		name   string
		result *diff.DiffResult
		want   int
	}{
		{
			name:   "first difference",
			result: &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 7)},
			want:   firstHunkHeader(linesWithDifference(20, 7)),
		},
		{
			name:   "difference clamped to final viewport",
			result: &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 19)},
			want:   clampedHeader(linesWithDifference(20, 19), 12),
		},
		{
			name:   "no textual difference",
			result: &diff.DiffResult{Lines: make([]diff.DiffLine, 20)},
			want:   0,
		},
		{
			name:   "binary difference",
			result: &diff.DiffResult{Binary: true, ContentDiff: true, Lines: linesWithDifference(20, 7)},
			want:   0,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			model := New([]diff.Session{{Result: test.result}}, config.Host{}, nil, nil, 100, 12)
			if model.scroll != test.want {
				t.Fatalf("initial scroll = %d, want %d", model.scroll, test.want)
			}
		})
	}
}

func TestRefreshScrollsToFirstDifference(t *testing.T) {
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: make([]diff.DiffLine, 20)},
		}},
		Height: 12,
		scroll: 3,
	}

	model, _ = model.Update(MsgRefreshed{Sessions: []diff.Session{{
		Result: &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 8)},
	}}})
	if model.scroll != firstHunkHeader(linesWithDifference(20, 8)) {
		t.Fatalf("refresh scroll = %d, want first hunk header at %d", model.scroll, firstHunkHeader(linesWithDifference(20, 8)))
	}
}

func TestSessionReloadScrollsToFirstDifferenceWhenActive(t *testing.T) {
	model := Model{
		sessions: []diff.Session{
			{Result: &diff.DiffResult{ContentDiff: true, Lines: make([]diff.DiffLine, 20)}},
			{Result: &diff.DiffResult{ContentDiff: true, Lines: make([]diff.DiffLine, 20)}},
		},
		Height: 13,
		scroll: 3,
	}

	model, _ = model.Update(MsgSessionReloaded{
		SessionIdx: 0,
		Result:     &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 9)},
	})
	if model.scroll != clampedHeader(linesWithDifference(20, 9), 13) {
		t.Fatalf("active session reload scroll = %d, want first hunk header at %d", model.scroll, clampedHeader(linesWithDifference(20, 9), 13))
	}

	model.scroll = 4
	model, _ = model.Update(MsgSessionReloaded{
		SessionIdx: 1,
		Result:     &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 12)},
	})
	if model.scroll != 4 {
		t.Fatalf("inactive session reload changed scroll to %d, want 4", model.scroll)
	}
}

func TestFileNavigationScrollsToFirstDifference(t *testing.T) {
	model := Model{
		sessions: []diff.Session{
			{Result: &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 2)}},
			{Result: &diff.DiffResult{ContentDiff: true, Lines: linesWithDifference(20, 7)}},
		},
		Height: 13,
		scroll: 5,
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyTab})
	if model.activeIdx != 1 {
		t.Fatalf("Tab selected file %d, want 1", model.activeIdx)
	}
	if model.scroll != clampedHeader(linesWithDifference(20, 7), 13) {
		t.Fatalf("Tab scroll = %d, want first hunk header at %d", model.scroll, clampedHeader(linesWithDifference(20, 7), 13))
	}

	model.scroll = 5
	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyShiftTab})
	if model.activeIdx != 0 {
		t.Fatalf("Shift+Tab selected file %d, want 0", model.activeIdx)
	}
	if model.scroll != clampedHeader(linesWithDifference(20, 2), 13) {
		t.Fatalf("Shift+Tab scroll = %d, want first hunk header at %d", model.scroll, clampedHeader(linesWithDifference(20, 2), 13))
	}
}

func linesWithDifference(total, changed int) []diff.DiffLine {
	lines := make([]diff.DiffLine, total)
	if changed >= 0 && changed < len(lines) {
		lines[changed].Kind = diff.LineRemoved
	}
	return lines
}

func TestHunkJumpLandsOnRunStart(t *testing.T) {
	// Flattened change: equal, removed, added, equal, removed — two hunks.
	// Pad with equals so clampScroll can leave the viewport on a hunk start.
	lines := []diff.DiffLine{
		{Text: "a", Kind: diff.LineEqual},
		{Text: "old", Kind: diff.LineRemoved},
		{Text: "new", Kind: diff.LineAdded},
		{Text: "b", Kind: diff.LineEqual},
		{Text: "gone", Kind: diff.LineRemoved},
	}
	for i := 0; i < 20; i++ {
		lines = append(lines, diff.DiffLine{Text: "pad", Kind: diff.LineEqual})
	}
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 12,
		scroll: 0,
	}

	headers := hunkHeaders(lines)
	if len(headers) < 2 {
		t.Fatalf("need two hunk headers, got %v", headers)
	}

	model, _ = model.handleKey(keyMsg("]"))
	if model.scroll != headers[0] {
		t.Fatalf("] jumped to %d, want first hunk header at %d", model.scroll, headers[0])
	}

	model, _ = model.handleKey(keyMsg("]"))
	if model.scroll != headers[1] {
		t.Fatalf("] jumped to %d, want second hunk header at %d", model.scroll, headers[1])
	}

	model, _ = model.handleKey(keyMsg("["))
	if model.scroll != headers[0] {
		t.Fatalf("[ jumped to %d, want previous hunk header at %d", model.scroll, headers[0])
	}
}

func TestEnterExpandsFirstVisibleFold(t *testing.T) {
	lines := linesWithDifference(20, 7)
	ids := diff.FoldableGapIDs(lines, diff.DefaultContext)
	if len(ids) == 0 {
		t.Fatal("fixture has no foldable gaps")
	}
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: 24,
		scroll: 0,
	}
	if _, ok := model.firstVisibleFold(); !ok {
		t.Fatal("expected a fold at the top of the viewport")
	}

	model, _ = model.handleKey(tea.KeyMsg{Type: tea.KeyEnter})
	if !model.gapExpanded(ids[0]) {
		t.Fatalf("Enter did not expand gap %d", ids[0])
	}

	model, _ = model.handleKey(keyMsg("c"))
	if model.gapExpanded(ids[0]) {
		t.Fatal("c did not collapse expanded folds")
	}

	model, _ = model.handleKey(keyMsg("c"))
	for _, id := range ids {
		if !model.gapExpanded(id) {
			t.Fatalf("c did not expand gap %d", id)
		}
	}

	model, _ = model.handleKey(keyMsg("h"))
	if model.gapExpanded(ids[0]) {
		t.Fatal("h did not collapse the visible expanded gap")
	}
}

func firstHunkHeader(lines []diff.DiffLine) int {
	hs := hunkHeaders(lines)
	if len(hs) == 0 {
		return 0
	}
	return hs[0]
}

func hunkHeaders(lines []diff.DiffLine) []int {
	var out []int
	for i, row := range diff.Flatten(lines, diff.DefaultContext, nil) {
		if row.Kind == diff.DisplayHunkHeader {
			out = append(out, i)
		}
	}
	return out
}

func clampedHeader(lines []diff.DiffLine, height int) int {
	model := Model{
		sessions: []diff.Session{{
			Result: &diff.DiffResult{ContentDiff: true, Lines: lines},
		}},
		Height: height,
	}
	want := firstHunkHeader(lines)
	max := model.totalLines() - model.viewportHeight()
	if max < 0 {
		max = 0
	}
	if want > max {
		return max
	}
	return want
}

func keyMsg(key string) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
}
