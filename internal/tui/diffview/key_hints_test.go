package diffview

import (
	"errors"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestDiffKeyHintsUsePrimaryStyle(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	for _, tc := range []struct {
		name  string
		model Model
		keys  []string
	}{
		{"normal", Model{Width: 500}, []string{"[Tab]", "[[]", "[]]", "[Enter]", "[s/S]", "[q]"}},
		{"scope", Model{Width: 500, scopeSet: true, sessions: []diff.Session{{Result: &diff.DiffResult{ContentDiff: true}}}}, []string{"[i]", "[s/S]", "[r]", "[q]"}},
		{"errors", Model{Width: 500, showErrors: true}, []string{"[e/q]"}},
		{"disconnected", Model{Width: 500, disconnected: errors.New("connection lost")}, []string{"[q]"}},
		{"sync result", Model{Width: 500, syncStatus: "1 error [e] to view"}, []string{"[e]"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got := tc.model.renderStatus(nil)
			for _, key := range tc.keys {
				if !strings.Contains(got, styles.Dir.Render(key)) {
					t.Fatalf("key %q not primary: %q", key, got)
				}
			}
		})
	}
	empty := Model{Width: 500, scopeSet: true}
	empty.scope.Pairs = 2
	emptyStatus := empty.renderStatus(nil)
	for _, key := range []string{"[i]", "[r]", "[q]"} {
		if !strings.Contains(emptyStatus, styles.Dir.Render(key)) {
			t.Fatalf("empty comparison key %q not primary: %q", key, emptyStatus)
		}
	}
	for _, key := range []string{"[s/S]", "[Tab]", "[Space]"} {
		if strings.Contains(ansi.Strip(emptyStatus), key) {
			t.Fatalf("empty comparison shows unavailable key %q: %q", key, emptyStatus)
		}
	}

	m := Model{}
	header := m.renderErrorListRows(1, 100)[0]
	if !strings.Contains(header, styles.Dir.Render("[e]")) || !strings.Contains(header, styles.Dir.Render("[q]")) {
		t.Fatalf("error panel hints: %q", header)
	}
	m.syncing = true
	m.syncTotal = 5
	m.Width = 200
	if got := ansi.Strip(m.renderStatus(nil)); !strings.Contains(got, "syncing [") {
		t.Fatalf("progress bar changed: %q", got)
	}
}
