package browser

import (
	"context"
	"fmt"
	"path"
	"sync"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
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
	session *string
}

// MsgRemoteChildrenLoaded is emitted after expanding a remote directory.
type MsgRemoteChildrenLoaded struct {
	Host       config.Host
	ID         uint64
	session    *string
	ParentPath string
	Children   []*fs.FileEntry
	Err        error
}

// StartRemote switches the right pane to host and starts loading its root.
func (m *Model) StartRemote(host config.Host) tea.Cmd {
	return m.startRemote(host, nil)
}

// RetryRemote repeats a failed load while pinning the certificate shown in the
// trust prompt for the first reconnect.
func (m *Model) RetryRemote(host config.Host, challenge tlstrust.Challenge) tea.Cmd {
	return m.startRemote(host, &challenge)
}

func (m *Model) startRemote(host config.Host, required *tlstrust.Challenge) tea.Cmd {
	if m.remoteSession == nil {
		root := m.WorkDir
		m.remoteSession = &root
	}
	sameHost := m.remoteHost != nil && m.remoteHost.Name == host.Name
	closeCmd := m.CloseRemote()
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
	id := m.remoteLoadID
	m.remoteTracker = loading.NewTracker(m.remoteStatus)
	load := loadRemoteCmd(host, m.remoteTracker.Context(), id, m.remoteSession, m.trust, required, m.classifier, m.config, m.WorkDir)
	if closeCmd == nil {
		return load
	}
	return tea.Sequence(closeCmd, load)
}

func loadRemoteCmd(host config.Host, parent context.Context, id uint64, session *string, trust *tlstrust.Manager, required *tlstrust.Challenge, classifier *fs.Classifier, cfg *config.MergedConfig, workDir string) tea.Cmd {
	return func() tea.Msg {
		root := remoteRoot(host)
		msg := MsgRemoteLoaded{Host: host, Root: root, ID: id, session: session}
		ctx, cancel := context.WithTimeout(parent, 30*time.Second)
		defer cancel()

		conn, err := remote.Connect(ctx, host, trust, required)
		if err != nil {
			log.Error("remote browser connect failed", "host", host.Name, "hostname", host.Hostname, "err", err)
			msg.Err = fmt.Errorf("connect to %s: %w", host.Hostname, err)
			return msg
		}
		// Until this command hands back a successful result, cancellation owns
		// the connection too. In particular it must interrupt a stalled listing.
		closed := make(chan struct{})
		cancelClose := context.AfterFunc(ctx, func() {
			defer close(closed)
			if err := conn.Close(); err != nil {
				log.Error("close cancelled browser load", "err", err)
			}
		})
		detach := sync.OnceFunc(func() {
			if !cancelClose() {
				<-closed
			}
		})
		defer detach()
		entries, readErr := conn.ReadDir(root)
		detach()
		err = ctx.Err()
		if err == nil {
			err = readErr
		}
		if err == nil {
			err = conn.Err()
		}
		if err != nil {
			log.Error("remote browser root read failed", "host", host.Name, "remote", root, "err", err)
			_ = conn.Close()
			msg.Err = fmt.Errorf("read %s: %w", root, err)
			return msg
		}
		classifierModel := Model{WorkDir: workDir, classifier: classifier, config: cfg}
		entries, err = classifierModel.classifyRemote(entries, host)
		if err != nil {
			log.Error("remote browser classification failed", "remote", root, "err", err)
			if closeErr := conn.Close(); closeErr != nil {
				log.Error("close failed remote browser load", "err", closeErr)
			}
			msg.Err = fmt.Errorf("classify %s: %w", root, err)
			return msg
		}
		for _, e := range entries {
			e.Depth = 0
		}
		msg.Conn, msg.Entries = conn, entries
		return msg
	}
}

func readRemoteDirCmd(conn remote.Client, host config.Host, id uint64, session *string, parentPath string, classifier *fs.Classifier, cfg *config.MergedConfig, workDir string) tea.Cmd {
	return func() tea.Msg {
		children, err := conn.ReadDir(parentPath)
		if err != nil {
			log.Error("remote browser directory read failed", "remote", parentPath, "err", err)
			return MsgRemoteChildrenLoaded{Host: host, ID: id, session: session, ParentPath: parentPath, Err: fmt.Errorf("read %s: %w", parentPath, err)}
		}
		classifierModel := Model{WorkDir: workDir, classifier: classifier, config: cfg}
		children, err = classifierModel.classifyRemote(children, host)
		if err != nil {
			log.Error("remote browser classification failed", "remote", parentPath, "err", err)
			return MsgRemoteChildrenLoaded{Host: host, ID: id, session: session, ParentPath: parentPath, Err: fmt.Errorf("classify %s: %w", parentPath, err)}
		}
		return MsgRemoteChildrenLoaded{Host: host, ID: id, session: session, ParentPath: parentPath, Children: children}
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
	if !m.AcceptsRemoteResult(msg) {
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
	if !m.AcceptsRemoteChildrenResult(msg) {
		return
	}
	m.remoteReading = false
	idx := m.remoteIndexByPath(msg.ParentPath)
	if idx < 0 {
		return
	}
	parent := m.remoteEntries[idx]
	revealChildren := m.remoteCurrent() == parent
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
	visibleChildren := 0
	for _, child := range msg.Children {
		if m.visible(child) {
			visibleChildren++
		}
	}
	parentVisible := indexEntry(m.visibleRemoteEntries(), parent)
	if revealChildren && visibleChildren > 0 && parentVisible >= 0 {
		viewportHeight := m.viewportHeight()
		if visibleChildren+1 >= viewportHeight {
			m.remoteOffset = parentVisible
		} else {
			minimumOffset := parentVisible + visibleChildren - viewportHeight + 1
			if m.remoteOffset < minimumOffset {
				m.remoteOffset = minimumOffset
			}
			if m.remoteOffset > parentVisible {
				m.remoteOffset = parentVisible
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

func (m Model) visibleRemoteEntries() []*fs.FileEntry {
	entries := make([]*fs.FileEntry, 0, len(m.remoteEntries))
	for _, entry := range m.remoteEntries {
		if m.visible(entry) {
			entries = append(entries, entry)
		}
	}
	return entries
}

func (m Model) remoteCurrent() *fs.FileEntry {
	entries := m.visibleRemoteEntries()
	if m.remoteCursor < 0 || m.remoteCursor >= len(entries) {
		return nil
	}
	return entries[m.remoteCursor]
}
