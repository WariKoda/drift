package browser

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/styles"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestHighlightMatchUsesRuneOffsetsAfterCaseConversion(t *testing.T) {
	got := ansi.Strip(highlightMatch("xȺ", "ⱥ", fs.EntryFile))
	if got != "xȺ" {
		t.Fatalf("highlightMatch returned %q, want %q", got, "xȺ")
	}
}

func TestVisibilityHintStyles(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	for _, hidden := range []bool{false, true} {
		for _, ignored := range []bool{false, true} {
			got := HelpText(hidden, ignored, 200)
			for _, tc := range []struct {
				key, label string
				active     bool
			}{{".", "hidden", hidden}, {"I", "ignored", ignored}} {
				style := styles.Muted
				if tc.active {
					style = styles.ActiveHint
				}
				want := styles.Dir.Render("["+tc.key+"]") + style.Render(tc.label)
				if !strings.Contains(got, want) {
					t.Fatalf("missing styled hint %q in %q", want, got)
				}
			}
			if strings.Contains(ansi.Strip(got), ":on") || strings.Contains(ansi.Strip(got), ":off") {
				t.Fatal("redundant state labels")
			}
		}
	}
}

func TestAdaptiveKeyHintsFitWithoutSplitting(t *testing.T) {
	for width := 0; width <= 200; width++ {
		for _, got := range []string{HelpText(true, false, width), PreviewHelpText(width)} {
			plain := ansi.Strip(got)
			if lipgloss.Width(got) > width {
				t.Fatalf("width %d: %q", width, got)
			}
			if width >= len("[?]help") && !strings.Contains(plain, "[?]help") {
				t.Fatalf("width %d lost help: %q", width, plain)
			}
			if strings.Count(plain, "[") != strings.Count(plain, "]") {
				t.Fatalf("partial hint: %q", plain)
			}
		}
	}
}

func TestStatusKeepsHelpWithLongMessage(t *testing.T) {
	for _, width := range []int{20, 40, 60, 80, 120, 200} {
		m := Model{Width: width, statusMsg: strings.Repeat("very long message ", 30)}
		got := m.renderStatus(nil)
		if lipgloss.Width(got) != width || !strings.Contains(ansi.Strip(got), "[?]help") {
			t.Fatalf("width %d: %q", width, got)
		}
	}
}

func TestActivePaneLabelsAndPreview(t *testing.T) {
	m := Model{Width: 121, Height: 24, WorkDir: "/project", remoteHost: &config.Host{Name: "test"}, remoteRoot: "/srv"}
	for _, pane := range []PaneSide{PaneLocal, PaneRemote} {
		m.activePane = pane
		left, right := m.paneWidths()
		got := ansi.Strip(m.renderPaneLabels(left, right))
		want := "▶ LOCAL"
		if pane == PaneRemote {
			want = "▶ REMOTE"
		}
		if !strings.Contains(got, want) || strings.Count(got, "▶") != 1 {
			t.Fatalf("active label: %q", got)
		}
		if lipgloss.Width(got) != m.Width || lipgloss.Width(m.renderPaneSep(left, right)) != m.Width {
			t.Fatal("pane geometry changed")
		}
		m.preview = filePreview{active: true, source: pane}
		got = ansi.Strip(m.renderPaneLabels(left, right))
		if !strings.Contains(got, want) || !strings.Contains(got, "PREVIEW") {
			t.Fatalf("preview stole pane label: %q", got)
		}
		m.preview.active = false
	}
}

func TestLocalEmptyReasons(t *testing.T) {
	for _, tc := range []struct {
		name         string
		entries      []*fs.FileEntry
		filter, want string
	}{
		{name: "empty", want: "Folder is empty"},
		{name: "filter", entries: []*fs.FileEntry{{Name: "file.txt"}}, filter: "absent", want: "No filter matches"},
		{name: "hidden", entries: []*fs.FileEntry{{Name: ".env", Class: fs.PathClass{Hidden: true}}}, want: "Hidden by visibility"},
		{name: "ignored", entries: []*fs.FileEntry{{Name: "cache", Class: fs.PathClass{Ignored: true}}}, want: "Hidden by visibility"},
		{name: "fixed", entries: []*fs.FileEntry{{Name: ".git", Class: fs.PathClass{HardExcluded: true}}}, want: "Only excluded"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			m := Model{entries: tc.entries, filter: tc.filter}
			got := ansi.Strip(m.renderLocalRow(m.filteredEntries(), 0, 80))
			if !strings.Contains(got, tc.want) {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
			if strings.TrimSpace(m.renderLocalRow(nil, 1, 80)) != "" {
				t.Fatal("empty message repeated")
			}
		})
	}
}

func TestRemoteEmptyReasons(t *testing.T) {
	m := Model{remoteHost: &config.Host{Name: "test"}, remoteEntries: []*fs.FileEntry{{Name: ".env", Class: fs.PathClass{Hidden: true}}}}
	if got := ansi.Strip(m.renderRemoteRow(0, 80)); !strings.Contains(got, "Hidden by visibility") {
		t.Fatal(got)
	}
	m.remoteEntries = nil
	if got := ansi.Strip(m.renderRemoteRow(0, 80)); strings.Contains(got, "Folder is empty") {
		t.Fatal("unconnected host shown as empty")
	}
	m.remoteLoading = true
	m.remoteStatus = "Connecting"
	if got := ansi.Strip(m.renderRemoteRow(0, 80)); !strings.Contains(got, "Connecting") {
		t.Fatal(got)
	}
}

func TestFinderEmptyReasons(t *testing.T) {
	for _, tc := range []struct {
		f    finder
		want string
	}{
		{finder{}, "No searchable files"},
		{finder{hidden: 2}, "Files hidden"},
		{finder{rel: []string{"file.txt"}, query: "absent"}, "No filter matches"},
	} {
		m := Model{Width: 80, Height: 24, finder: tc.f}
		if got := ansi.Strip(m.renderFinder()); !strings.Contains(got, tc.want) {
			t.Fatalf("want %q: %s", tc.want, got)
		}
	}
}

func TestFinderCarriesHiddenCount(t *testing.T) {
	root := t.TempDir()
	writeVisibilityFile(t, root+"/.env", "hidden")
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("f")})
	msg := buildFinderIndexCmd(root, m.classifier, false, false, m.finder.id, m.remoteSession)()
	m, _ = m.Update(msg)
	if m.finder.hidden != 1 || len(m.finder.rel) != 0 {
		t.Fatalf("finder: %+v", m.finder)
	}
}
