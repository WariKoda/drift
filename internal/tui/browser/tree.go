package browser

import (
	"context"
	"path/filepath"
	"strings"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/WariKoda/drift/internal/fs"
)

// expandAt schedules a directory read without sharing live entries with the command.
func (m *Model) expandAt(i int) tea.Cmd {
	entry := m.entries[i]
	if entry.Kind != fs.EntryDir || entry.Expanded {
		return nil
	}
	m.cancelLocalRefresh()
	ctx, cancel := context.WithCancel(context.Background())
	request := &localReadRequest{session: m.remoteSession, path: entry.Path, cancel: cancel}
	if m.localReads == nil {
		m.localReads = make(map[string]*localReadRequest)
	}
	m.localReads[entry.Path] = request
	entry.Expanded, entry.Children = true, nil
	classifier := m.classifier
	return func() tea.Msg {
		defer cancel()
		entries, err := readLocalDirectory(ctx, classifier, request.path)
		return MsgLocalLoaded{request: request, entries: entries, err: err}
	}
}

// collapseAt collapses the directory at entries[i], removing all its descendants.
func (m *Model) collapseAt(i int) {
	entry := m.entries[i]
	if entry.Kind != fs.EntryDir || !entry.Expanded {
		return
	}
	m.cancelLocalRefresh()
	for path, request := range m.localReads {
		if path == entry.Path || strings.HasPrefix(path, entry.Path+string(filepath.Separator)) {
			request.cancel()
			delete(m.localReads, path)
		}
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
