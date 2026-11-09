package diffview

import (
	"fmt"

	"github.com/WariKoda/drift/internal/styles"
)

// longLineBytes marks a changed line as too long to read as a diff, as in
// minified bundles. Such files open as a summary until [o] shows the diff.
const longLineBytes = 4096

// longHidden reports whether a file's diff is replaced by its long-line summary.
func (m Model) longHidden(index int) bool {
	if index < 0 || index >= len(m.sessions) || m.longOpen[index] {
		return false
	}
	result := m.sessions[index].Result
	return result != nil && !result.Binary && result.LongestChange() >= longLineBytes
}

func (m Model) longNote(index int) []string {
	return []string{
		"  " + styles.Warn.Render(fmt.Sprintf("A changed line is %d bytes long; the diff is hidden to keep it readable.", m.sessions[index].Result.LongestChange())),
		"  " + styles.KeyHints("[o] show the diff anyway. Sync is not affected.", styles.Muted),
	}
}

// toggleLong shows or hides the diff of a file with long lines.
func (m *Model) toggleLong(index int) {
	if index < 0 || index >= len(m.sessions) {
		return
	}
	result := m.sessions[index].Result
	if result == nil || result.Binary || result.LongestChange() < longLineBytes {
		return
	}
	if m.longOpen == nil {
		m.longOpen = make(map[int]bool)
	}
	m.longOpen[index] = !m.longOpen[index]
	m.foldRevision++
	m.prepareDisplayRows()
	if !m.continuous() {
		m.scrollToFirstDifference()
	}
}
