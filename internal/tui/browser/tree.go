package browser

import (
	"strings"

	"github.com/WariKoda/drift/internal/fs"
)

// expandAt expands the directory at entries[i], inserting its children into the flat list.
// Returns error if ReadDir fails; entries is not modified in that case.
func (m *Model) expandAt(i int) error {
	entry := m.entries[i]
	if entry.Kind != fs.EntryDir || entry.Expanded {
		return nil
	}

	children, err := fs.ReadDir(entry.Path)
	if err != nil {
		return err
	}
	children, err = m.classifyLocal(children)
	if err != nil {
		return err
	}
	for _, c := range children {
		c.Depth = entry.Depth + 1
		c.Parent = entry
	}
	entry.Children = children
	entry.Expanded = true

	// Insert children after position i
	newEntries := make([]*fs.FileEntry, 0, len(m.entries)+len(children))
	newEntries = append(newEntries, m.entries[:i+1]...)
	newEntries = append(newEntries, children...)
	newEntries = append(newEntries, m.entries[i+1:]...)
	m.entries = newEntries

	// Keep the directory and as many visible children as the viewport allows.
	visibleChildren := 0
	for _, child := range children {
		if m.visible(child) && (m.filter == "" || contains(toLower(child.Name), toLower(m.filter))) {
			visibleChildren++
		}
	}
	entryVisible := indexEntry(m.filteredEntries(), entry)
	if visibleChildren > 0 && entryVisible >= 0 {
		viewportHeight := m.viewportHeight()
		if visibleChildren+1 >= viewportHeight {
			m.offset = entryVisible
		} else {
			minimumOffset := entryVisible + visibleChildren - viewportHeight + 1
			if m.offset < minimumOffset {
				m.offset = minimumOffset
			}
			if m.offset > entryVisible {
				m.offset = entryVisible
			}
		}
		m.clampLocalOffset()
	}
	return nil
}

// collapseAt collapses the directory at entries[i], removing all its descendants.
func (m *Model) collapseAt(i int) {
	entry := m.entries[i]
	if entry.Kind != fs.EntryDir || !entry.Expanded {
		return
	}
	entry.Expanded = false

	// Find the end of descendants: all consecutive entries with depth > entry.Depth
	end := i + 1
	for end < len(m.entries) && m.entries[end].Depth > entry.Depth {
		end++
	}

	m.entries = append(m.entries[:i+1], m.entries[end:]...)
}

func indexEntry(entries []*fs.FileEntry, entry *fs.FileEntry) int {
	for i, candidate := range entries {
		if candidate == entry {
			return i
		}
	}
	return -1
}

func (m Model) localIndex(entry *fs.FileEntry) int {
	return indexEntry(m.entries, entry)
}

// filteredEntries is the single local projection used by rendering and input.
func (m Model) filteredEntries() []*fs.FileEntry {
	lower := toLower(m.filter)
	var result []*fs.FileEntry
	for _, entry := range m.entries {
		if !m.visible(entry) {
			continue
		}
		if lower == "" || contains(toLower(entry.Name), lower) {
			result = append(result, entry)
		}
	}
	return result
}

func toLower(s string) string {
	return strings.ToLower(s)
}

func contains(s, sub string) bool {
	return len(sub) == 0 || (len(s) >= len(sub) && indexStr(s, sub) >= 0)
}

func indexStr(s, sub string) int {
	if len(sub) == 0 {
		return 0
	}
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}
