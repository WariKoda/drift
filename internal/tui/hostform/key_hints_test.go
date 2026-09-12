package hostform

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestMappingDeleteHintsPreservePath(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })

	m := New(config.ScopeGlobal, "", 120, 30)
	m.mappings = []config.Mapping{{Local: "plugins/[q]", Remote: "plugins"}}
	m.mapConfirmDel = true
	view := m.viewMappingList()
	for _, key := range []string{"[y]", "[any]", "[n]", "[Esc]"} {
		if !strings.Contains(view, styles.Dir.Render(key)) {
			t.Errorf("mapping hint does not use directory style for %s", key)
		}
	}
	if strings.Contains(view, styles.Dir.Render("[q]")) {
		t.Error("mapping path was styled as a key hint")
	}
	if !strings.Contains(ansi.Strip(view), `Delete "plugins/[q]"?  [y]yes  [any]cancel`) {
		t.Error("delete confirmation text changed")
	}
}
