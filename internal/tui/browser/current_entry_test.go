package browser

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestCurrentEntryKeepsNeutralFocusAndMark(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	for _, tc := range []struct {
		name   string
		kind   fs.EntryKind
		suffix string
	}{
		{"file", fs.EntryFile, ""}, {"directory", fs.EntryDir, "/"}, {"link", fs.EntrySymlink, "@"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			entry := &fs.FileEntry{Name: "example", Path: "/example", Kind: tc.kind}
			selection := fs.NewSelectionState()
			selection.Toggle(entry.Path)
			m := Model{entries: []*fs.FileEntry{entry}, remoteEntries: []*fs.FileEntry{entry}, Selection: selection}
			active := m.renderLocalRow(m.entries, 0, 80)
			expected := styles.CurrentEntry.Render(entry.Name + tc.suffix)
			if !strings.Contains(active, expected) {
				t.Fatalf("current name lacks focus style: %q", active)
			}
			if !strings.Contains(ansi.Strip(active), "●") {
				t.Fatal("mark lost")
			}
			if !strings.HasPrefix(ansi.Strip(active), "› ") {
				t.Fatal("cursor marker lost on marked row")
			}
			m.activePane = PaneRemote
			inactive := m.renderLocalRow(m.entries, 0, 80)
			if strings.Contains(inactive, styles.CurrentEntry.Render(entry.Name+tc.suffix)) {
				t.Fatalf("inactive pane recolored: %q", inactive)
			}
			short := m.renderEntry(entry, true, 9, nil, "")
			if !strings.Contains(short, styles.CurrentEntry.Render("exa…")) {
				t.Fatalf("truncated name lacks focus style: %q", short)
			}
		})
	}
}
