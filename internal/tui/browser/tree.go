package browser

import (
	"github.com/nibra180/drift/internal/fs"
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

// parentIndex returns the index of the nearest ancestor of entries[i] in the flat list,
// or -1 if the entry is at the root level (depth 0).
func (m Model) parentIndex(i int) int {
	depth := m.entries[i].Depth
	if depth == 0 {
		return -1
	}
	for j := i - 1; j >= 0; j-- {
		if m.entries[j].Depth < depth {
			return j
		}
	}
	return -1
}

// filteredEntries returns entries matching the current filter string (case-insensitive).
// If filter is empty, returns all entries.
func (m Model) filteredEntries() []*fs.FileEntry {
	if m.filter == "" {
		return m.entries
	}
	lower := toLower(m.filter)
	var result []*fs.FileEntry
	for _, e := range m.entries {
		if contains(toLower(e.Name), lower) {
			result = append(result, e)
		}
	}
	return result
}

func toLower(s string) string {
	b := []byte(s)
	for i, c := range b {
		if c >= 'A' && c <= 'Z' {
			b[i] = c + 32
		}
	}
	return string(b)
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
