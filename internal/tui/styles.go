// Package tui holds the Bubble Tea root model, which routes messages to one
// active screen at a time; the screens are its subpackages. All style
// definitions live in internal/styles, so the screen packages can share them
// without importing tui.
package tui

import "github.com/WariKoda/drift/internal/styles"

// Convenience aliases so tui package code can write e.g. StyleHeader.Render(...)
var (
	StyleHeader    = styles.Header
	StyleMuted     = styles.Muted
	StyleStatusKey = styles.Key
	StyleWarn      = styles.Warn
	StyleError     = styles.Err
	StyleMarked    = styles.Marked
	StyleFile      = styles.File
	StyleSep       = styles.Sep
	StyleCursorRow = styles.CursorRow
)
