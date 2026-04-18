// Package tui re-exports styles for convenience within the tui package itself.
// All style definitions live in internal/styles to avoid circular imports.
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
