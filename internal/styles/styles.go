// Package styles defines all lipgloss colors and styles used across drift's TUI.
// Keeping them in a separate package avoids circular imports between tui and tui/browser.
package styles

import "github.com/charmbracelet/lipgloss"

// Colors used by the app. They are initialised from the selected palette in init.
var (
	ColorDir      lipgloss.TerminalColor
	ColorFile     lipgloss.TerminalColor
	ColorMarked   lipgloss.TerminalColor
	ColorSymlink  lipgloss.TerminalColor
	ColorCursorBg lipgloss.TerminalColor
	ColorHeader   lipgloss.TerminalColor
	ColorMuted    lipgloss.TerminalColor
	ColorSep      lipgloss.TerminalColor
	ColorBadgeFg  lipgloss.TerminalColor
	ColorBadgeBg  lipgloss.TerminalColor
	ColorKey      lipgloss.TerminalColor
	ColorWarn     lipgloss.TerminalColor
	ColorError    lipgloss.TerminalColor
	ColorMatch    lipgloss.TerminalColor
	ColorAccent   lipgloss.TerminalColor

	ColorDiffAddedBg     lipgloss.TerminalColor
	ColorDiffAddedText   lipgloss.TerminalColor
	ColorDiffRemovedBg   lipgloss.TerminalColor
	ColorDiffRemovedText lipgloss.TerminalColor
	ColorDiffMissingBg   lipgloss.TerminalColor
)

var (
	Dir       lipgloss.Style
	File      lipgloss.Style
	Link      lipgloss.Style
	Marked    lipgloss.Style
	Muted     lipgloss.Style
	Header    lipgloss.Style
	Sep       lipgloss.Style
	Badge     lipgloss.Style
	Key       lipgloss.Style
	Warn      lipgloss.Style
	Err       lipgloss.Style
	CursorRow lipgloss.Style
	Accent    lipgloss.Style
)

func init() {
	ApplyPalette(loadPalette())
}

// ApplyPalette replaces the app palette and rebuilds all shared styles.
func ApplyPalette(p Palette) {
	ColorDir = p.Dir
	ColorFile = p.File
	ColorMarked = p.Marked
	ColorSymlink = p.Symlink
	ColorCursorBg = p.CursorBg
	ColorHeader = p.Header
	ColorMuted = p.Muted
	ColorSep = p.Sep
	ColorBadgeFg = p.BadgeFg
	ColorBadgeBg = p.BadgeBg
	ColorKey = p.Key
	ColorWarn = p.Warn
	ColorError = p.Error
	ColorMatch = p.Match
	ColorAccent = p.Accent
	ColorDiffAddedBg = p.DiffAddedBg
	ColorDiffAddedText = p.DiffAddedText
	ColorDiffRemovedBg = p.DiffRemovedBg
	ColorDiffRemovedText = p.DiffRemovedText
	ColorDiffMissingBg = p.DiffMissingBg

	Dir = lipgloss.NewStyle().Foreground(ColorDir).Bold(true)
	File = lipgloss.NewStyle().Foreground(ColorFile)
	Link = lipgloss.NewStyle().Foreground(ColorSymlink)
	Marked = lipgloss.NewStyle().Foreground(ColorMarked).Bold(true)
	Muted = lipgloss.NewStyle().Foreground(ColorMuted)
	Header = lipgloss.NewStyle().Foreground(ColorHeader).Bold(true)
	Sep = lipgloss.NewStyle().Foreground(ColorSep)
	Badge = lipgloss.NewStyle().Foreground(ColorBadgeFg).Background(ColorBadgeBg).Padding(0, 1)
	Key = lipgloss.NewStyle().Foreground(ColorKey).Bold(true)
	Warn = lipgloss.NewStyle().Foreground(ColorWarn)
	Err = lipgloss.NewStyle().Foreground(ColorError)
	CursorRow = lipgloss.NewStyle().Background(ColorCursorBg)
	Accent = lipgloss.NewStyle().Foreground(ColorAccent).Bold(true)
}
