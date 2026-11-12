// Package browser implements the yazi-like local file browser TUI component.
package browser

import (
	"context"
	"os"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/pathmap"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
)

// PaneSide identifies which side of the split browser receives navigation keys.
type PaneSide int

const (
	PaneLocal PaneSide = iota
	PaneRemote
)

// Model is the bubbletea sub-model for the file browser screen.
type Model struct {
	// tree state
	WorkDir     string
	entries     []*fs.FileEntry // flat visible list
	cursor      int
	offset      int // scroll offset into entries
	localReload *localReadRequest
	localReads  map[string]*localReadRequest

	// selection
	Selection       *fs.SelectionState
	RemoteSelection *fs.SelectionState

	// visual selection mode
	visualMode      bool
	visualStartPath string
	visualPane      PaneSide

	// filter
	filterMode bool
	filter     string

	// visibility and path classification
	classifier  *fs.Classifier
	config      *config.MergedConfig
	showHidden  bool
	showIgnored bool

	// fuzzy file finder overlay
	finder    finder
	finderSeq uint64

	// help overlay
	showHelp bool

	// file preview shown over the pane opposite the active tree
	preview filePreview

	// terminal dimensions (set by root app on WindowSizeMsg)
	Width  int
	Height int

	// side-by-side remote browser state
	activePane           PaneSide
	remoteHost           *config.Host
	remoteConn           remote.Client
	editConn             remote.Client
	editPending          bool
	editToken            *string
	remoteRoot           string
	remoteEntries        []*fs.FileEntry
	remoteCursor         int
	remoteOffset         int
	remoteLoading        bool
	remoteReading        bool
	remoteReadPath       string
	remotePreviewReading bool
	remotePreviewID      uint64 // generation of the dispatched read, independent of the current selection
	remotePreviewCancel  context.CancelFunc
	remotePreviewConn    remote.Client
	remoteStatus         string
	remoteLoadID         uint64
	remoteSession        *string // unique identity across browser/project replacements
	remoteTracker        *progress.Tracker
	trust                *tlstrust.Manager

	// status message (transient)
	statusMsg string

	// projectName is the registry display name, shown in the header when set.
	projectName string

	// mouse
	mouseEnabled bool
	clicks       mouse.ClickTracker
}

// New creates a browser Model for the given directory.
// Initial width/height will be overwritten by the first WindowSizeMsg.
func New(workDir string) (Model, error) {
	classifier, err := fs.NewClassifier(context.Background(), workDir)
	if err != nil {
		return Model{}, err
	}
	m := Model{
		WorkDir:         workDir,
		remoteSession:   &workDir,
		classifier:      classifier,
		Selection:       fs.NewSelectionState(),
		RemoteSelection: fs.NewSelectionState(),
		Width:           80,
		Height:          24,
	}
	if err := m.reload(); err != nil {
		return Model{}, err
	}
	return m, nil
}

// Init satisfies the tea.Model interface (root app calls this).
func (m Model) Init() tea.Cmd {
	return nil
}

// SetMouseEnabled remembers whether drift should restore mouse reporting after
// temporarily releasing the mouse to the terminal while a preview is open.
func (m *Model) SetMouseEnabled(enabled bool) {
	m.mouseEnabled = enabled
}

// SetConfig provides mappings and initial visibility preferences.
func (m *Model) SetConfig(cfg *config.MergedConfig) {
	m.config = cfg
	if cfg != nil {
		m.showHidden = cfg.UI.ShowHidden
		m.showIgnored = cfg.UI.ShowIgnored
	}
	m.clampScroll()
}

func (m Model) visible(entry *fs.FileEntry) bool {
	return !entry.Class.HardExcluded &&
		(m.showHidden || !entry.Class.Hidden) &&
		(m.showIgnored || !entry.Class.Ignored)
}

func (m *Model) classifyRemote(entries []*fs.FileEntry, host config.Host) ([]*fs.FileEntry, error) {
	if m.classifier == nil {
		return entries, nil
	}
	mapper := pathmap.New(m.WorkDir, nil, host)
	if m.config != nil {
		mapper = pathmap.New(m.config.ProjectRoot, m.config.Mappings, host)
	}
	candidates := make([]fs.ClassifyCandidate, 0, len(entries))
	candidateIndexes := make([]int, 0, len(entries))
	remoteHidden := make(map[int]bool, len(entries))
	for i, entry := range entries {
		if rel, relErr := filepath.Rel(remoteRoot(host), entry.Path); relErr == nil {
			remoteHidden[i] = fs.IsHiddenPath(filepath.ToSlash(rel))
		}
		localPath, mapErr := mapper.RemoteToLocal(entry.Path)
		if mapErr != nil {
			entry.Unmapped = true
			rel, relErr := filepath.Rel(remoteRoot(host), entry.Path)
			if relErr != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
				continue
			}
			localPath = filepath.Join(m.WorkDir, filepath.FromSlash(rel))
		}
		candidates = append(candidates, fs.ClassifyCandidate{Path: localPath, IsDir: entry.Kind == fs.EntryDir})
		candidateIndexes = append(candidateIndexes, i)
	}
	classes, err := m.classifier.ClassifyBatch(context.Background(), candidates)
	if err != nil {
		return nil, err
	}
	for i, class := range classes {
		index := candidateIndexes[i]
		entry := entries[index]
		class.Hidden = class.Hidden || remoteHidden[index]
		if entry.Unmapped {
			class.Ignored = false
		}
		entry.Class = class
	}
	return entries, nil
}

// SetTrustManager provides certificate trust for new FTPS connections.
func (m *Model) SetTrustManager(trust *tlstrust.Manager) {
	m.trust = trust
}

// StartsNetworkOperation reports whether key could perform network I/O and
// must be blocked while another global activity is running.
func (m Model) StartsNetworkOperation(key tea.KeyMsg) bool {
	switch key.String() {
	case keyS, keyAt:
		return true
	case keyE:
		return m.activePane == PaneRemote
	case keyP:
		return m.activePane == PaneRemote && !m.preview.active
	case keyJ, keyK, keyDown, keyUp, keyG, keyShiftG:
		return m.preview.active && m.preview.source == PaneRemote
	case keyR:
		return m.activePane == PaneRemote && m.remoteHost != nil
	case keyL, keyRight, keyEnter:
		return m.activePane == PaneRemote
	default:
		return false
	}
}

// LoadingActivity reports an in-flight remote connection/root listing.
func (m Model) LoadingActivity() (string, *progress.Tracker, bool) {
	if !m.remoteLoading {
		return "", nil, false
	}
	if m.remoteHost == nil {
		return "Connecting to remote…", m.remoteTracker, true
	}
	return "Connecting to " + m.remoteHost.Name + "…", m.remoteTracker, true
}

// AcceptsRemoteResult reports whether a root-load result is still current.
func (m Model) AcceptsRemoteResult(msg MsgRemoteLoaded) bool {
	return m.remoteLoading && msg.session == m.remoteSession && msg.ID == m.remoteLoadID && m.remoteHost != nil && m.remoteHost.Name == msg.Host.Name
}

// AcceptsRemoteChildrenResult reports whether a directory result belongs to
// the active remote connection generation.
func (m Model) AcceptsRemoteChildrenResult(msg MsgRemoteChildrenLoaded) bool {
	return m.remoteReading && msg.session == m.remoteSession && msg.ID == m.remoteLoadID &&
		msg.conn == m.remoteConn && msg.tracker == m.remoteTracker &&
		m.remoteHost != nil && m.remoteHost.Name == msg.Host.Name
}

// RemoteHost returns the host currently assigned to the remote pane.
func (m Model) RemoteHost() (config.Host, bool) {
	if m.remoteHost == nil {
		return config.Host{}, false
	}
	return *m.remoteHost, true
}

// CancelRemote aborts an in-flight listing and ignores its result. A directory
// read also returns a command to close its connection off the UI thread.
func (m *Model) CancelRemote() tea.Cmd {
	var conn remote.Client
	if m.remoteTracker != nil {
		m.remoteTracker.Cancel()
		m.remoteTracker = nil
	}
	m.remoteLoadID++
	if m.remoteReading {
		if idx := m.remoteIndexByPath(m.remoteReadPath); idx >= 0 {
			m.remoteEntries[idx].Expanded = false
		}
		m.remoteReading = false
		m.remoteReadPath = ""
		conn = m.remoteConn
		m.remoteConn = nil
		m.remoteStatus = "Remote directory read cancelled. Press [r] to reconnect."
		if m.preview.source == PaneRemote {
			m.preview.generation++
			m.preview.loading = false
			m.preview.waiting = false
		}
	}
	if m.remoteLoading {
		m.remoteLoading = false
		m.remoteStatus = "Cancelled"
	}
	if conn == nil {
		return nil
	}
	return func() tea.Msg {
		if err := conn.Close(); err != nil {
			log.Error("close cancelled browser directory read", "err", err)
		}
		return nil
	}
}

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
	m.clampScroll()
	m.clampRemoteScroll()
	m.layoutPreview(true)
}

// SetStatus sets a transient status message (e.g. error from a previous screen).
func (m *Model) SetStatus(msg string) {
	m.statusMsg = msg
}

// SetProjectName sets the registry display name shown in the header.
func (m *Model) SetProjectName(name string) {
	m.projectName = name
}

// viewportHeight returns the number of lines available for entries.
// See the layout constants in view.go for the row budget.
func (m Model) viewportHeight() int {
	h := m.Height - headerLines - footerLines
	if h < 1 {
		return 1
	}
	return h
}

// finderViewportHeight returns the rows available for finder results.
func (m Model) finderViewportHeight() int {
	h := m.Height - finderHeaderLines - finderFooterLines
	if h < 1 {
		return 1
	}
	return h
}

// clampLocalOffset keeps the local scroll offset in range without dragging it
// back to the cursor. The wheel moves the viewport on its own, so clampScroll —
// which exists to follow the cursor — must not run after it.
func (m *Model) clampLocalOffset() {
	m.offset = clampOffset(m.offset, len(m.filteredEntries()), m.viewportHeight())
}

// clampRemoteOffset is clampLocalOffset for the remote pane.
func (m *Model) clampRemoteOffset() {
	m.remoteOffset = clampOffset(m.remoteOffset, len(m.visibleRemoteEntries()), m.viewportHeight())
}

// clampOffset bounds a scroll offset to [0, count-vh].
func clampOffset(offset, count, vh int) int {
	max := count - vh
	if max < 0 {
		max = 0
	}
	if offset > max {
		offset = max
	}
	if offset < 0 {
		offset = 0
	}
	return offset
}

// clampScroll ensures cursor and offset are within bounds.
func (m *Model) clampScroll() {
	count := len(m.filteredEntries())
	if count == 0 {
		m.cursor = 0
		m.offset = 0
		return
	}
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= count {
		m.cursor = count - 1
	}
	vh := m.viewportHeight()
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.cursor >= m.offset+vh {
		m.offset = m.cursor - vh + 1
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

// clampRemoteScroll ensures the remote cursor and offset are within bounds.
func (m *Model) clampRemoteScroll() {
	count := len(m.visibleRemoteEntries())
	if count == 0 {
		m.remoteCursor = 0
		m.remoteOffset = 0
		return
	}
	if m.remoteCursor < 0 {
		m.remoteCursor = 0
	}
	if m.remoteCursor >= count {
		m.remoteCursor = count - 1
	}
	vh := m.viewportHeight()
	if m.remoteCursor < m.remoteOffset {
		m.remoteOffset = m.remoteCursor
	}
	if m.remoteCursor >= m.remoteOffset+vh {
		m.remoteOffset = m.remoteCursor - vh + 1
	}
	if m.remoteOffset < 0 {
		m.remoteOffset = 0
	}
}

// reload initializes a browser before it is attached to the event loop.
func (m *Model) reload() error {
	entries, err := readLocalDirectory(context.Background(), m.classifier, m.WorkDir)
	if err != nil {
		return err
	}
	m.entries = entries
	m.clampScroll()
	return nil
}

// CloseRemote detaches the connection immediately and closes it off the UI thread.
func (m *Model) CloseRemote() tea.Cmd {
	conn := m.remoteConn
	if conn == nil {
		conn = m.editConn
	}
	m.editConn, m.editToken, m.editPending = nil, nil, false
	_ = m.cancelRemotePreview(false)
	m.remoteConn = nil
	m.CancelRemote()
	m.remoteReading = false
	m.remotePreviewReading = false
	if conn == nil {
		return nil
	}
	return func() tea.Msg {
		if err := conn.Close(); err != nil {
			log.Error("close remote browser connection", "err", err)
		}
		return nil
	}
}

// ClearRemote closes the current connection and removes the assigned host and
// remote tree. Use it when the surrounding project identity is removed.
func (m *Model) ClearRemote() tea.Cmd {
	cmd := m.CloseRemote()
	m.remoteHost = nil
	m.remoteRoot = ""
	m.remoteEntries = nil
	m.remoteCursor = 0
	m.remoteOffset = 0
	m.remoteLoading = false
	m.remoteStatus = ""
	if m.RemoteSelection != nil {
		m.RemoteSelection.Clear()
	}
	return cmd
}

// Connection returns the connection currently owned by the browser.
func (m Model) Connection() remote.Client { return m.remoteConn }

// ConnectionLost invalidates pending reads without discarding visible entries.
func (m *Model) ConnectionLost(conn remote.Client, err error) bool {
	if conn == nil || m.remoteConn != conn || err == nil {
		return false
	}
	_ = m.cancelRemotePreview(false)
	if m.remoteTracker != nil {
		m.remoteTracker.Cancel()
		m.remoteTracker = nil
	}
	if m.remoteReading {
		if idx := m.remoteIndexByPath(m.remoteReadPath); idx >= 0 {
			m.remoteEntries[idx].Expanded = false
		}
	}
	m.remoteReadPath = ""
	m.remoteLoadID++
	m.remoteReading = false
	m.remotePreviewReading = false
	m.remoteStatus = "Remote disconnected: " + sanitizePreviewError(err) + ". Press [r] to reconnect."
	if m.preview.source == PaneRemote {
		m.preview.generation++
		m.preview.loading = false
		m.preview.waiting = false
	}
	return true
}

func (m Model) remoteBusy() bool {
	return m.editPending || m.remoteLoading || m.remoteReading || m.remotePreviewReading
}

func (m Model) cachedPathClass(path string, remote bool) (fs.ClassifyCandidate, fs.PathClass, bool) {
	if remote {
		if m.remoteHost == nil {
			return fs.ClassifyCandidate{}, fs.PathClass{}, false
		}
		mapper := pathmap.New(m.WorkDir, nil, *m.remoteHost)
		if m.config != nil {
			mapper = pathmap.New(m.config.ProjectRoot, m.config.Mappings, *m.remoteHost)
		}
		localPath, err := mapper.RemoteToLocal(path)
		if err != nil {
			return fs.ClassifyCandidate{}, fs.PathClass{}, false
		}
		path = localPath
	}
	candidate := fs.ClassifyCandidate{Path: path}
	if class, ok := m.classifier.CachedClassify(path, false); ok {
		return candidate, class, true
	}
	if class, ok := m.classifier.CachedClassify(path, true); ok {
		candidate.IsDir = true
		return candidate, class, true
	}
	return candidate, fs.PathClass{}, false
}

func (m Model) hiddenSelectionCount() int {
	count := 0
	if m.Selection != nil {
		for localPath := range m.Selection.Marked {
			_, class, ok := m.cachedPathClass(localPath, false)
			if !ok || (!m.showHidden && class.Hidden) || (!m.showIgnored && class.Ignored) || class.HardExcluded {
				count++
			}
		}
	}
	if m.remoteHost == nil || m.RemoteSelection == nil {
		return count
	}
	for remotePath := range m.RemoteSelection.Marked {
		_, class, ok := m.cachedPathClass(remotePath, true)
		if !ok || (!m.showHidden && class.Hidden) || (!m.showIgnored && class.Ignored) || class.HardExcluded {
			count++
		}
	}
	return count
}

func (m Model) absWorkDir() string {
	abs, err := filepath.Abs(m.WorkDir)
	if err != nil {
		return m.WorkDir
	}
	home, _ := os.UserHomeDir()
	if home != "" && strings.HasPrefix(abs, home) {
		return "~" + abs[len(home):]
	}
	return abs
}
