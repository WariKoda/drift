package browser

import (
	"context"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	tea "github.com/charmbracelet/bubbletea"
)

type localReadRequest struct {
	session *string
	path    string
	refresh bool
	cancel  context.CancelFunc
}

// MsgLocalLoaded carries a local tree result, also while another screen is open.
type MsgLocalLoaded struct {
	request    *localReadRequest
	entries    []*fs.FileEntry
	classifier *fs.Classifier
	err        error
}

// RefreshLocal rebuilds the visible tree and ignore snapshot off the UI thread.
// The previous tree remains usable until a current result succeeds.
func (m *Model) RefreshLocal() tea.Cmd {
	m.CancelLocal()
	expanded := make(map[string]bool)
	for _, entry := range m.entries {
		if entry.Kind == fs.EntryDir && entry.Expanded {
			expanded[entry.Path] = true
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	request := &localReadRequest{session: m.remoteSession, path: m.WorkDir, refresh: true, cancel: cancel}
	m.localReload = request
	return func() tea.Msg {
		defer cancel()
		result := MsgLocalLoaded{request: request}
		result.classifier, result.err = fs.NewClassifier(ctx, request.path)
		if result.err == nil {
			result.entries, result.err = readLocalTree(ctx, result.classifier, request.path, expanded)
		}
		return result
	}
}

func (m *Model) cancelLocalRefresh() {
	if m.localReload != nil {
		m.localReload.cancel()
		m.localReload = nil
	}
}

// CancelLocal invalidates pending local results without waiting for filesystem I/O.
func (m *Model) CancelLocal() {
	m.cancelLocalRefresh()
	for _, request := range m.localReads {
		request.cancel()
		for _, entry := range m.entries {
			if entry.Path == request.path {
				entry.Expanded = false
				break
			}
		}
	}
	m.localReads = nil
}

func readLocalDirectory(ctx context.Context, classifier *fs.Classifier, path string) ([]*fs.FileEntry, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	entries, err := fs.ReadDir(path)
	if err != nil {
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if classifier == nil {
		return entries, nil
	}
	candidates := make([]fs.ClassifyCandidate, len(entries))
	for i, entry := range entries {
		candidates[i] = fs.ClassifyCandidate{Path: entry.Path, IsDir: entry.Kind == fs.EntryDir}
	}
	classes, err := classifier.ClassifyBatch(ctx, candidates)
	if err != nil {
		return nil, err
	}
	for i, entry := range entries {
		entry.Class = classes[i]
	}
	return entries, nil
}

func readLocalTree(ctx context.Context, classifier *fs.Classifier, root string, expanded map[string]bool) ([]*fs.FileEntry, error) {
	var entries []*fs.FileEntry
	var walk func(string, int, *fs.FileEntry) error
	walk = func(path string, depth int, parent *fs.FileEntry) error {
		children, err := readLocalDirectory(ctx, classifier, path)
		if err != nil {
			return err
		}
		if parent != nil {
			parent.Children, parent.Expanded = children, true
		}
		for _, entry := range children {
			entry.Depth, entry.Parent = depth, parent
			entries = append(entries, entry)
			if entry.Kind == fs.EntryDir && expanded[entry.Path] && !entry.Class.HardExcluded {
				if err := walk(entry.Path, depth+1, entry); err != nil {
					return err
				}
			}
		}
		return nil
	}
	if err := walk(root, 0, nil); err != nil {
		return nil, err
	}
	return entries, nil
}

func (m *Model) applyLocalLoaded(msg MsgLocalLoaded) tea.Cmd {
	request := msg.request
	if request == nil || request.session != m.remoteSession {
		return nil
	}
	if request.refresh {
		if m.localReload != request {
			return nil
		}
		m.localReload = nil
	} else {
		if m.localReads[request.path] != request {
			return nil
		}
		delete(m.localReads, request.path)
	}
	request.cancel()
	if msg.err != nil {
		if !request.refresh {
			for _, entry := range m.entries {
				if entry.Path == request.path {
					entry.Expanded = false
					break
				}
			}
		}
		log.Error("browser local read failed", "path", request.path, "err", msg.err)
		m.statusMsg = "Local read failed: " + msg.err.Error()
		return nil
	}
	cursorPath := ""
	if visible := m.filteredEntries(); m.cursor >= 0 && m.cursor < len(visible) {
		cursorPath = visible[m.cursor].Path
	}
	if request.refresh {
		m.entries, m.classifier = msg.entries, msg.classifier
		m.statusMsg = "Refreshed"
	} else {
		for i, entry := range m.entries {
			if entry.Path != request.path || !entry.Expanded {
				continue
			}
			for _, child := range msg.entries {
				child.Depth, child.Parent = entry.Depth+1, entry
			}
			entry.Children = msg.entries
			next := make([]*fs.FileEntry, 0, len(m.entries)+len(msg.entries))
			next = append(next, m.entries[:i+1]...)
			next = append(next, msg.entries...)
			m.entries = append(next, m.entries[i+1:]...)
			if cursorPath == entry.Path {
				visibleChildren := 0
				for _, child := range msg.entries {
					if m.visible(child) && (m.filter == "" || contains(toLower(child.Name), toLower(m.filter))) {
						visibleChildren++
					}
				}
				entryVisible := indexEntry(m.filteredEntries(), entry)
				if visibleChildren > 0 && entryVisible >= 0 {
					m.offset = max(m.offset, entryVisible+visibleChildren-m.viewportHeight()+1)
					m.offset = min(m.offset, entryVisible)
					m.clampLocalOffset()
				}
			}
			break
		}
	}
	for i, entry := range m.filteredEntries() {
		if entry.Path == cursorPath {
			m.cursor = i
			break
		}
	}
	m.clampScroll()
	return m.schedulePreview()
}
