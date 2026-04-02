// Package styles defines all lipgloss colors and styles used across drift's TUI.
// Keeping them in a separate package avoids circular imports between tui and tui/browser.
package styles

import "github.com/charmbracelet/lipgloss"

// Adaptive colors — degrade gracefully in both light and dark terminals.
var (
	ColorDir      = lipgloss.AdaptiveColor{Light: "#1E88E5", Dark: "#89B4FA"}
	ColorFile     = lipgloss.AdaptiveColor{Light: "#212121", Dark: "#CDD6F4"}
	ColorMarked   = lipgloss.AdaptiveColor{Light: "#E65100", Dark: "#FAB387"}
	ColorSymlink  = lipgloss.AdaptiveColor{Light: "#00897B", Dark: "#94E2D5"}
	ColorCursorBg = lipgloss.AdaptiveColor{Light: "#E3F2FD", Dark: "#313244"}
	ColorHeader   = lipgloss.AdaptiveColor{Light: "#6200EA", Dark: "#CBA6F7"}
	ColorMuted    = lipgloss.AdaptiveColor{Light: "#9E9E9E", Dark: "#6C7086"}
	ColorSep      = lipgloss.AdaptiveColor{Light: "#BDBDBD", Dark: "#45475A"}
	ColorBadgeFg  = lipgloss.AdaptiveColor{Light: "#FFFFFF", Dark: "#1E1E2E"}
	ColorBadgeBg  = lipgloss.AdaptiveColor{Light: "#1E88E5", Dark: "#89B4FA"}
	ColorKey      = lipgloss.AdaptiveColor{Light: "#37474F", Dark: "#BAC2DE"}
	ColorWarn     = lipgloss.AdaptiveColor{Light: "#F57F17", Dark: "#F9E2AF"}
	ColorError    = lipgloss.AdaptiveColor{Light: "#B71C1C", Dark: "#F38BA8"}
	ColorMatch    = lipgloss.AdaptiveColor{Light: "#FF6F00", Dark: "#F9E2AF"}
)

var (
	Dir       = lipgloss.NewStyle().Foreground(ColorDir).Bold(true)
	File      = lipgloss.NewStyle().Foreground(ColorFile)
	Link      = lipgloss.NewStyle().Foreground(ColorSymlink)
	Marked    = lipgloss.NewStyle().Foreground(ColorMarked).Bold(true)
	Muted     = lipgloss.NewStyle().Foreground(ColorMuted)
	Header    = lipgloss.NewStyle().Foreground(ColorHeader).Bold(true)
	Sep       = lipgloss.NewStyle().Foreground(ColorSep)
	Badge     = lipgloss.NewStyle().Foreground(ColorBadgeFg).Background(ColorBadgeBg).Padding(0, 1)
	Key       = lipgloss.NewStyle().Foreground(ColorKey).Bold(true)
	Warn      = lipgloss.NewStyle().Foreground(ColorWarn)
	Err       = lipgloss.NewStyle().Foreground(ColorError)
	CursorRow = lipgloss.NewStyle().Background(ColorCursorBg)
)
