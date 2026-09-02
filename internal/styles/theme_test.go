package styles

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestOmarchyThemeFilePrefersCurrentStateLocation(t *testing.T) {
	stateDir := t.TempDir()
	configDir := t.TempDir()
	t.Setenv("DRIFT_THEME_FILE", "")
	t.Setenv("XDG_STATE_HOME", stateDir)
	t.Setenv("XDG_CONFIG_HOME", configDir)

	stateFile := writeThemeFile(t, stateDir)
	writeThemeFile(t, configDir)

	if got := omarchyThemeFile(); got != stateFile {
		t.Fatalf("omarchyThemeFile() = %q, want %q", got, stateFile)
	}
}

func TestOmarchyThemeFileSupportsLegacyConfigLocation(t *testing.T) {
	stateDir := t.TempDir()
	configDir := t.TempDir()
	t.Setenv("DRIFT_THEME_FILE", "")
	t.Setenv("XDG_STATE_HOME", stateDir)
	t.Setenv("XDG_CONFIG_HOME", configDir)

	configFile := writeThemeFile(t, configDir)

	if got := omarchyThemeFile(); got != configFile {
		t.Fatalf("omarchyThemeFile() = %q, want %q", got, configFile)
	}
}

func TestLoadOmarchyPaletteSupportsCurrentSchema(t *testing.T) {
	file := filepath.Join(t.TempDir(), "colors.toml")
	content := `
accent = "#7daea3"
selection = "#504945"
muted = "#665c54"
background = "#282828"
foreground = "#d4be98"
red = "#ea6962"
yellow = "#d8a657"
green = "#a9b665"
cyan = "#89b482"
blue = "#7daea3"
magenta = "#d3869b"
`
	if err := os.WriteFile(file, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}

	palette, err := loadOmarchyPalette(file)
	if err != nil {
		t.Fatal(err)
	}

	assertColor(t, "cursor background", palette.CursorBg, "#504945")
	assertColor(t, "muted", palette.Muted, "#665c54")
	assertColor(t, "error", palette.Error, "#ea6962")
	assertColor(t, "marked", palette.Marked, "#d8a657")
	assertColor(t, "symlink", palette.Symlink, "#89b482")
	assertColor(t, "header", palette.Header, "#d3869b")
}

func TestANSIPaletteCursorDiffersFromTerminalBackground(t *testing.T) {
	assertColor(t, "cursor background", ansiPalette().CursorBg, "8")
}

func writeThemeFile(t *testing.T, baseDir string) string {
	t.Helper()
	file := filepath.Join(baseDir, "omarchy", "current", "theme", "colors.toml")
	if err := os.MkdirAll(filepath.Dir(file), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(file, []byte("background = \"#000000\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	return file
}

func assertColor(t *testing.T, name string, got lipgloss.TerminalColor, want string) {
	t.Helper()
	if got != lipgloss.Color(want) {
		t.Errorf("%s = %v, want %s", name, got, want)
	}
}
