package styles

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestKeyHintsPrimaryColorAndPlainText(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	for _, base := range []lipgloss.Style{Muted, Err, Warn, File} {
		for _, tc := range []struct {
			text string
			keys []string
		}{
			{"[i]include ignored  [s/S]sync", []string{"[i]", "[s/S]"}},
			{"[[]/[]]hunk  [Tab/←/→] select", []string{"[[]", "[]]", "[Tab/←/→]"}},
			{" [Ctrl+S / Enter on last]save [Esc]cancel", []string{"[Ctrl+S / Enter on last]", "[Esc]"}},
			{"plain description", nil},
		} {
			got := KeyHints(tc.text, base)
			if ansi.Strip(got) != tc.text {
				t.Fatalf("changed text: %q", got)
			}
			for _, key := range tc.keys {
				if !strings.Contains(got, Dir.Render(key)) {
					t.Fatalf("missing primary key %q: %q", key, got)
				}
			}
			if lipgloss.Width(got) != lipgloss.Width(tc.text) {
				t.Fatal("changed width")
			}
		}
	}
}
