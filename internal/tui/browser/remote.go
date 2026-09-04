package browser

import (
	"context"
	"fmt"
	"path"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tui/loading"
	tea "github.com/charmbracelet/bubbletea"
)

// MsgRemoteLoaded is emitted after connecting to a host and loading its root.
type MsgRemoteLoaded struct {
	Host    config.Host
	Root    string
	Conn    remote.Client
	Entries []*fs.FileEntry
	Err     error
	ID      uint64
}

// MsgRemoteChildrenLoaded is emitted after expanding a remote directory.
type MsgRemoteChildrenLoaded struct {
	ParentPath string
	Children   []*fs.FileEntry
	Err        error
}

// StartRemote switches the right pane to host and starts loading its root.
func (m *Model) StartRemote(host config.Host) tea.Cmd {
	sameHost := m.remoteHost != nil && m.remoteHost.Name == host.Name
	m.CloseRemote()
	m.remoteHost = &host
	m.remoteConn = nil
	m.remoteRoot = remoteRoot(host)
	m.remoteEntries = nil
	if m.RemoteSelection == nil {
		m.RemoteSelection = fs.NewSelectionState()
	} else if !sameHost {
		m.RemoteSelection.Clear()
	}
	m.remoteCursor = 0
	m.remoteOffset = 0
	m.remoteLoading = true
	m.remoteReading = false
	m.remoteStatus = "Connecting to " + host.Name + "…"
	m.activePane = PaneRemote
	m.remoteLoadID++
	id := m.remoteLoadID
	m.remoteTracker = loading.NewTracker(m.remoteStatus)
	return loadRemoteCmd(host, m.remoteTracker.Context(), id)
}

func loadRemoteCmd(host config.Host, parent context.Context, id uint64) tea.Cmd {
	return func() tea.Msg {
		root := remoteRoot(host)
		ctx, cancel := context.WithTimeout(parent, 30*time.Second)
		defer cancel()

		conn, err := remote.Connect(ctx, host)
		if err != nil {
			log.Error("remote browser connect failed", "host", host.Name, "hostname", host.Hostname, "err", err)
			return MsgRemoteLoaded{Host: host, Root: root, Err: fmt.Errorf("connect to %s: %w", host.Hostname, err), ID: id}
		}
		if err := ctx.Err(); err != nil {
			_ = conn.Close()
			return MsgRemoteLoaded{Host: host, Root: root, Err: err, ID: id}
		}
		entries, err := conn.ReadDir(root)
		if err != nil {
			log.Error("remote browser root read failed", "host", host.Name, "remote", root, "err", err)
			_ = conn.Close()
			return MsgRemoteLoaded{Host: host, Root: root, Err: fmt.Errorf("read %s: %w", root, err), ID: id}
		}
		for _, e := range entries {
			e.Depth = 0
		}
		return MsgRemoteLoaded{Host: host, Root: root, Conn: conn, Entries: entries, ID: id}
	}
}

func readRemoteDirCmd(conn remote.Client, parentPath string) tea.Cmd {
	return func() tea.Msg {
		children, err := conn.ReadDir(parentPath)
		if err != nil {
			log.Error("remote browser directory read failed", "remote", parentPath, "err", err)
			return MsgRemoteChildrenLoaded{ParentPath: parentPath, Err: fmt.Errorf("read %s: %w", parentPath, err)}
		}
		return MsgRemoteChildrenLoaded{ParentPath: parentPath, Children: children}
	}
}

func remoteRoot(host config.Host) string {
	if host.RootPath == "" {
		return "/"
	}
	return path.Clean(host.RootPath)
}

func (m *Model) applyRemoteLoaded(msg MsgRemoteLoaded) {
	// Ignore stale connection results after the user picked another host or cancelled.
	if msg.ID != m.remoteLoadID || m.remoteHost == nil || m.remoteHost.Name != msg.Host.Name {
		if msg.Conn != nil {
			_ = msg.Conn.Close()
		}
		return
	}
	m.remoteLoading = false
	m.remoteTracker = nil
	if loading.IsCanceled(msg.Err) {
		m.remoteConn = nil
		m.remoteEntries = nil
		m.remoteStatus = "Cancelled"
		return
	}
	if msg.Err != nil {
		m.remoteConn = nil
		m.remoteEntries = nil
		m.remoteStatus = "Remote error: " + msg.Err.Error()
		return
	}
	m.remoteConn = msg.Conn
	m.remoteRoot = msg.Root
	m.remoteEntries = msg.Entries
	m.remoteStatus = "Remote connected: " + msg.Host.Name
	m.clampRemoteScroll()
}

func (m *Model) applyRemoteChildrenLoaded(msg MsgRemoteChildrenLoaded) {
	m.remoteReading = false
	idx := m.remoteIndexByPath(msg.ParentPath)
	if idx < 0 {
		return
	}
	parent := m.remoteEntries[idx]
	revealChildren := m.remoteCursor == idx
	parent.Expanded = false
	if msg.Err != nil {
		m.remoteStatus = "Remote error: " + msg.Err.Error()
		return
	}
	for _, child := range msg.Children {
		child.Depth = parent.Depth + 1
		child.Parent = parent
	}
	parent.Children = msg.Children
	parent.Expanded = true

	newEntries := make([]*fs.FileEntry, 0, len(m.remoteEntries)+len(msg.Children))
	newEntries = append(newEntries, m.remoteEntries[:idx+1]...)
	newEntries = append(newEntries, msg.Children...)
	newEntries = append(newEntries, m.remoteEntries[idx+1:]...)
	m.remoteEntries = newEntries
	m.remoteStatus = "Remote loaded: " + parent.Name

	// A remote read can finish after the user has moved elsewhere. Reveal the
	// children only while the directory that started the read is still active.
	if revealChildren && len(msg.Children) > 0 {
		viewportHeight := m.viewportHeight()
		if len(msg.Children)+1 >= viewportHeight {
			m.remoteOffset = idx
		} else {
			minimumOffset := idx + len(msg.Children) - viewportHeight + 1
			if m.remoteOffset < minimumOffset {
				m.remoteOffset = minimumOffset
			}
			if m.remoteOffset > idx {
				m.remoteOffset = idx
			}
		}
	}
	m.clampRemoteScroll()
}

func (m Model) remoteIndexByPath(p string) int {
	for i, e := range m.remoteEntries {
		if e.Path == p {
			return i
		}
	}
	return -1
}

func (m *Model) collapseRemoteAt(i int) {
	entry := m.remoteEntries[i]
	if entry.Kind != fs.EntryDir || !entry.Expanded {
		return
	}
	entry.Expanded = false
	end := i + 1
	for end < len(m.remoteEntries) && m.remoteEntries[end].Depth > entry.Depth {
		end++
	}
	m.remoteEntries = append(m.remoteEntries[:i+1], m.remoteEntries[end:]...)
}

func (m Model) remoteParentIndex(i int) int {
	depth := m.remoteEntries[i].Depth
	if depth == 0 {
		return -1
	}
	for j := i - 1; j >= 0; j-- {
		if m.remoteEntries[j].Depth < depth {
			return j
		}
	}
	return -1
}

func (m Model) remoteCurrent() *fs.FileEntry {
	if m.remoteCursor < 0 || m.remoteCursor >= len(m.remoteEntries) {
		return nil
	}
	return m.remoteEntries[m.remoteCursor]
}
