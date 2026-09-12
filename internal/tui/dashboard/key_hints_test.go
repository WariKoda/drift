package dashboard

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestKeyHintsUseDirectoryStyle(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })

	m := Model{Width: 120, Height: 24, entries: []entry{{
		proj:    project.Project{Name: "Project [n]", Path: "/missing/[q]"},
		missing: true,
	}}}
	for _, key := range []string{"[↵]", "[n]", "[e]", "[q]"} {
		if !strings.Contains(m.actionBar(), styles.Dir.Render(key)) {
			t.Errorf("action bar does not use directory style for %s", key)
		}
	}
	if !strings.HasSuffix(m.renderRow(0, 30), styles.Dir.Render("1")) {
		t.Error("quick-open key does not use directory style")
	}
	m, _ = m.chooseCurrent()
	status := m.footerBlock(func(s string) string { return s })[2]
	if !strings.Contains(status, styles.Dir.Render("[e]")) {
		t.Error("missing-path hint does not use directory style")
	}
	if strings.Contains(status, styles.Dir.Render("[q]")) {
		t.Error("path data was styled as a key hint")
	}
	if got := ansi.Strip(status); got != "Path not found: /missing/[q] — press [e] to fix" {
		t.Errorf("status text changed: %q", got)
	}
}
