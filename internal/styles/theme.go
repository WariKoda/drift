package styles

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"
	"github.com/charmbracelet/lipgloss"
)

const (
	themeAuto    = "auto"
	themeANSI    = "ansi"
	themeOmarchy = "omarchy"
	themeDefault = "default"
)

// Palette maps app-level semantic roles to terminal colours.
type Palette struct {
	Dir      lipgloss.TerminalColor
	File     lipgloss.TerminalColor
	Marked   lipgloss.TerminalColor
	Symlink  lipgloss.TerminalColor
	CursorBg lipgloss.TerminalColor
	Header   lipgloss.TerminalColor
	Muted    lipgloss.TerminalColor
	Sep      lipgloss.TerminalColor
	BadgeFg  lipgloss.TerminalColor
	BadgeBg  lipgloss.TerminalColor
	Key      lipgloss.TerminalColor
	Warn     lipgloss.TerminalColor
	Error    lipgloss.TerminalColor
	Match    lipgloss.TerminalColor
	Accent   lipgloss.TerminalColor

	DiffAddedBg     lipgloss.TerminalColor
	DiffAddedText   lipgloss.TerminalColor
	DiffRemovedBg   lipgloss.TerminalColor
	DiffRemovedText lipgloss.TerminalColor
	DiffMissingBg   lipgloss.TerminalColor
}

type omarchyColors struct {
	Accent              string `toml:"accent"`
	Cursor              string `toml:"cursor"`
	Foreground          string `toml:"foreground"`
	Background          string `toml:"background"`
	SelectionForeground string `toml:"selection_foreground"`
	SelectionBackground string `toml:"selection_background"`
	Color0              string `toml:"color0"`
	Color1              string `toml:"color1"`
	Color2              string `toml:"color2"`
	Color3              string `toml:"color3"`
	Color4              string `toml:"color4"`
	Color5              string `toml:"color5"`
	Color6              string `toml:"color6"`
	Color7              string `toml:"color7"`
	Color8              string `toml:"color8"`
	Color9              string `toml:"color9"`
	Color10             string `toml:"color10"`
	Color11             string `toml:"color11"`
	Color12             string `toml:"color12"`
	Color13             string `toml:"color13"`
	Color14             string `toml:"color14"`
	Color15             string `toml:"color15"`
}

func loadPalette() Palette {
	mode := strings.ToLower(strings.TrimSpace(os.Getenv("DRIFT_THEME")))
	if mode == "" {
		mode = themeAuto
	}

	switch mode {
	case themeDefault:
		return defaultPalette()
	case themeANSI:
		return ansiPalette()
	case themeOmarchy:
		if p, err := loadOmarchyPalette(omarchyThemeFile()); err == nil {
			return p
		}
		return defaultPalette()
	case themeAuto:
		if p, err := loadOmarchyPalette(omarchyThemeFile()); err == nil {
			return p
		}
		return ansiPalette()
	default:
		return defaultPalette()
	}
}

func omarchyThemeFile() string {
	if file := strings.TrimSpace(os.Getenv("DRIFT_THEME_FILE")); file != "" {
		return file
	}
	if xdg := strings.TrimSpace(os.Getenv("XDG_CONFIG_HOME")); xdg != "" {
		return filepath.Join(xdg, "omarchy", "current", "theme", "colors.toml")
	}
	if home, err := os.UserHomeDir(); err == nil && home != "" {
		return filepath.Join(home, ".config", "omarchy", "current", "theme", "colors.toml")
	}
	return ""
}

func defaultPalette() Palette {
	return Palette{
		Dir:      lipgloss.AdaptiveColor{Light: "#1E88E5", Dark: "#89B4FA"},
		File:     lipgloss.AdaptiveColor{Light: "#212121", Dark: "#CDD6F4"},
		Marked:   lipgloss.AdaptiveColor{Light: "#E65100", Dark: "#FAB387"},
		Symlink:  lipgloss.AdaptiveColor{Light: "#00897B", Dark: "#94E2D5"},
		CursorBg: lipgloss.AdaptiveColor{Light: "#E3F2FD", Dark: "#313244"},
		Header:   lipgloss.AdaptiveColor{Light: "#6200EA", Dark: "#CBA6F7"},
		Muted:    lipgloss.AdaptiveColor{Light: "#9E9E9E", Dark: "#6C7086"},
		Sep:      lipgloss.AdaptiveColor{Light: "#BDBDBD", Dark: "#45475A"},
		BadgeFg:  lipgloss.AdaptiveColor{Light: "#FFFFFF", Dark: "#1E1E2E"},
		BadgeBg:  lipgloss.AdaptiveColor{Light: "#1E88E5", Dark: "#89B4FA"},
		Key:      lipgloss.AdaptiveColor{Light: "#37474F", Dark: "#BAC2DE"},
		Warn:     lipgloss.AdaptiveColor{Light: "#F57F17", Dark: "#F9E2AF"},
		Error:    lipgloss.AdaptiveColor{Light: "#B71C1C", Dark: "#F38BA8"},
		Match:    lipgloss.AdaptiveColor{Light: "#FF6F00", Dark: "#F9E2AF"},
		Accent:   lipgloss.AdaptiveColor{Light: "#D79921", Dark: "#FABD2F"},

		DiffAddedBg:     lipgloss.AdaptiveColor{Light: "#E8F5E9", Dark: "#1B4332"},
		DiffAddedText:   lipgloss.AdaptiveColor{Light: "#1B5E20", Dark: "#A6E3A1"},
		DiffRemovedBg:   lipgloss.AdaptiveColor{Light: "#FFEBEE", Dark: "#4A1010"},
		DiffRemovedText: lipgloss.AdaptiveColor{Light: "#B71C1C", Dark: "#F38BA8"},
		DiffMissingBg:   lipgloss.AdaptiveColor{Light: "#F5F5F5", Dark: "#1E1E1E"},
	}
}

func ansiPalette() Palette {
	return Palette{
		Dir:      lipgloss.Color("4"),
		File:     lipgloss.Color("7"),
		Marked:   lipgloss.Color("3"),
		Symlink:  lipgloss.Color("6"),
		CursorBg: lipgloss.Color("0"),
		Header:   lipgloss.Color("4"),
		Muted:    lipgloss.Color("8"),
		Sep:      lipgloss.Color("8"),
		BadgeFg:  lipgloss.Color("0"),
		BadgeBg:  lipgloss.Color("4"),
		Key:      lipgloss.Color("7"),
		Warn:     lipgloss.Color("3"),
		Error:    lipgloss.Color("1"),
		Match:    lipgloss.Color("3"),
		Accent:   lipgloss.Color("3"),

		DiffAddedBg:     lipgloss.Color("2"),
		DiffAddedText:   lipgloss.Color("15"),
		DiffRemovedBg:   lipgloss.Color("1"),
		DiffRemovedText: lipgloss.Color("15"),
		DiffMissingBg:   lipgloss.Color("8"),
	}
}

func loadOmarchyPalette(file string) (Palette, error) {
	if file == "" {
		return Palette{}, fmt.Errorf("empty theme path")
	}
	var colors omarchyColors
	if _, err := toml.DecodeFile(file, &colors); err != nil {
		return Palette{}, err
	}
	return paletteFromOmarchy(colors)
}

func paletteFromOmarchy(c omarchyColors) (Palette, error) {
	background := normalizeHex(first(c.Background, "#000000"))
	foreground := normalizeHex(first(c.Foreground, c.Color7, "#ffffff"))
	accent := normalizeHex(first(c.Accent, c.Color4, foreground))
	muted := normalizeHex(first(c.Color8, c.Color0, foreground))
	red := normalizeHex(first(c.Color9, c.Color1, "#ff0000"))
	green := normalizeHex(first(c.Color10, c.Color2, "#00ff00"))
	yellow := normalizeHex(first(c.Color11, c.Color3, accent))
	magenta := normalizeHex(first(c.Color13, c.Color5, accent))
	cyan := normalizeHex(first(c.Color14, c.Color6, accent))

	return Palette{
		Dir:      lipgloss.Color(accent),
		File:     lipgloss.Color(foreground),
		Marked:   lipgloss.Color(yellow),
		Symlink:  lipgloss.Color(cyan),
		CursorBg: lipgloss.Color(mixHex(background, muted, 0.45)),
		Header:   lipgloss.Color(magenta),
		Muted:    lipgloss.Color(muted),
		Sep:      lipgloss.Color(muted),
		BadgeFg:  lipgloss.Color(background),
		BadgeBg:  lipgloss.Color(accent),
		Key:      lipgloss.Color(foreground),
		Warn:     lipgloss.Color(yellow),
		Error:    lipgloss.Color(red),
		Match:    lipgloss.Color(yellow),
		Accent:   lipgloss.Color(accent),

		DiffAddedBg:     lipgloss.Color(mixHex(background, green, 0.22)),
		DiffAddedText:   lipgloss.Color(green),
		DiffRemovedBg:   lipgloss.Color(mixHex(background, red, 0.22)),
		DiffRemovedText: lipgloss.Color(red),
		DiffMissingBg:   lipgloss.Color(mixHex(background, muted, 0.28)),
	}, nil
}

func first(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}

func normalizeHex(value string) string {
	value = strings.TrimSpace(value)
	value = strings.Trim(value, `"'`)
	if !strings.HasPrefix(value, "#") {
		value = "#" + value
	}
	return value
}

func mixHex(base, overlay string, amount float64) string {
	br, bg, bb, err := parseHexRGB(base)
	if err != nil {
		return base
	}
	or, og, ob, err := parseHexRGB(overlay)
	if err != nil {
		return base
	}
	if amount < 0 {
		amount = 0
	}
	if amount > 1 {
		amount = 1
	}
	return fmt.Sprintf("#%02x%02x%02x", mixChannel(br, or, amount), mixChannel(bg, og, amount), mixChannel(bb, ob, amount))
}

func mixChannel(base, overlay int, amount float64) int {
	return int(float64(base)*(1-amount) + float64(overlay)*amount + 0.5)
}

func parseHexRGB(value string) (r, g, b int, err error) {
	value = strings.TrimPrefix(normalizeHex(value), "#")
	if len(value) != 6 {
		return 0, 0, 0, fmt.Errorf("invalid hex color %q", value)
	}
	r64, err := strconv.ParseInt(value[0:2], 16, 0)
	if err != nil {
		return 0, 0, 0, err
	}
	g64, err := strconv.ParseInt(value[2:4], 16, 0)
	if err != nil {
		return 0, 0, 0, err
	}
	b64, err := strconv.ParseInt(value[4:6], 16, 0)
	if err != nil {
		return 0, 0, 0, err
	}
	return int(r64), int(g64), int(b64), nil
}
