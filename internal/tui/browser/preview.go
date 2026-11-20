package browser

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"image"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
	"unicode"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/imagefile"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/termimage"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

const (
	previewMaxBytes = 1 << 20
	// previewImageSide bounds decoded preview images; the terminal never needs
	// more pixels for one pane.
	previewImageSide = 2048
	previewDebounce  = 150 * time.Millisecond
	previewTimeout   = 30 * time.Second
)

var (
	errPreviewTooLarge = errors.New("file exceeds the 1 MiB preview limit")
	errPreviewBinary   = errors.New("binary files cannot be previewed")
	errPreviewNotFile  = errors.New("only regular files can be previewed")
)

type previewRequest struct {
	generation uint64
	source     PaneSide
	path       string
	size       int64
	force      bool
	host       config.Host
	remoteID   uint64
	session    *string
	// decodeImages is set when the terminal can show images, so image files
	// are read up to imagefile.MaxBytes and decoded.
	decodeImages bool
}

type msgPreviewDebounced struct {
	request previewRequest
}

type msgPreviewLoaded struct {
	request previewRequest
	lines   []string
	picture *previewPicture
	err     error
}

// previewPicture is a previewed image file. image is nil when the terminal
// cannot show images or decoding failed (err).
type previewPicture struct {
	info  imagefile.Info
	image image.Image
	err   error

	// The cell area last requested from the terminal and the placement it
	// returned. token rejects results of superseded requests.
	areaCols, areaRows int
	token              uint64
	placement          termimage.Placement
	rows               []string // placeholder text of placement
}

// MsgPreviewImageShown reaches the browser on any screen, so a placement that
// finishes while another screen is active is still recorded or freed.
type MsgPreviewImageShown = msgPreviewImageShown

type msgPreviewImageShown struct {
	generation uint64
	token      uint64
	placement  termimage.Placement
	err        error
}

// MsgPreviewLoaded exposes preview failures to the root model while keeping
// preview rendering state private to the browser package.
type MsgPreviewLoaded = msgPreviewLoaded

// Failure returns the preview read error, if any.
func (m msgPreviewLoaded) Failure() error { return m.err }

// Remote reports whether this result came from the FTPS/SFTP pane.
func (m msgPreviewLoaded) Remote() bool { return m.request.source == PaneRemote }

// Host returns the remote host snapshot attached to this preview request.
func (m msgPreviewLoaded) Host() (config.Host, bool) {
	if !m.Remote() {
		return config.Host{}, false
	}
	return m.request.host, true
}

type previewRow struct {
	lineNumber   int
	continuation bool
	text         string
}

type filePreview struct {
	active     bool
	source     PaneSide
	generation uint64
	loading    bool
	loaded     bool
	waiting    bool

	path        string
	targetPath  string
	message     string
	lines       []string
	picture     *previewPicture
	rows        []previewRow
	numberWidth int
	offset      int
	pending     previewRequest
}

func (m *Model) togglePreview() tea.Cmd {
	if m.preview.active {
		return m.disablePreview()
	}

	generation := m.preview.generation + 1
	m.preview = filePreview{
		active:     true,
		source:     m.activePane,
		generation: generation,
		message:    "Select a regular file to preview",
	}
	loadCmd := m.schedulePreview()
	if !m.mouseEnabled {
		return loadCmd
	}
	return tea.Batch(loadCmd, tea.DisableMouse)
}

func (m *Model) disablePreview() tea.Cmd {
	wasActive := m.preview.active
	closeCmd := tea.Batch(m.cancelRemotePreview(true), m.dropPreviewImage())
	generation := m.preview.generation + 1
	m.preview = filePreview{generation: generation}
	if strings.HasPrefix(m.statusMsg, "Preview failed: ") {
		m.statusMsg = ""
	}
	if wasActive && m.mouseEnabled {
		return tea.Batch(closeCmd, tea.EnableMouseCellMotion)
	}
	return closeCmd
}

func (m *Model) schedulePreview() tea.Cmd {
	if !m.preview.active {
		return nil
	}

	m.preview.generation++
	m.preview.loading = false
	m.preview.waiting = false
	m.preview.targetPath = ""

	request, ok := m.currentPreviewRequest(m.preview.generation)
	if !ok {
		if !m.preview.loaded {
			m.preview.path = ""
			m.preview.message = "Select a regular file to preview"
		}
		return nil
	}

	m.preview.loading = true
	m.preview.targetPath = request.path
	m.preview.pending = request
	if !m.preview.loaded {
		m.preview.message = "Loading preview…"
	}

	return tea.Tick(previewDebounce, func(time.Time) tea.Msg {
		return msgPreviewDebounced{request: request}
	})
}

func (m *Model) prepareRemotePreviewRefresh() {
	if !m.preview.active || m.preview.source != PaneRemote {
		return
	}

	m.preview.generation++
	request, ok := m.currentPreviewRequest(m.preview.generation)
	if !ok {
		m.preview.loading = false
		m.preview.waiting = false
		return
	}
	request.force = true
	m.preview.loading = true
	m.preview.waiting = true
	m.preview.targetPath = request.path
	m.preview.pending = request
	if !m.preview.loaded {
		m.preview.message = "Loading preview…"
	}
}

func (m Model) currentPreviewRequest(generation uint64) (previewRequest, bool) {
	var entry *fs.FileEntry
	switch m.preview.source {
	case PaneRemote:
		if m.remoteHost == nil {
			return previewRequest{}, false
		}
		entry = m.remoteCurrent()
	default:
		entries := m.filteredEntries()
		if m.cursor >= 0 && m.cursor < len(entries) {
			entry = entries[m.cursor]
		}
	}
	if entry == nil || entry.Kind != fs.EntryFile || !entry.Mode.IsRegular() {
		return previewRequest{}, false
	}

	filePath := entry.Path
	if m.preview.source == PaneLocal {
		if absolute, err := filepath.Abs(filePath); err == nil {
			filePath = absolute
		}
	}
	request := previewRequest{
		generation: generation,
		source:     m.preview.source,
		path:       filePath,
		size:       entry.Size,
		session:    m.remoteSession,

		decodeImages: m.images.Enabled(),
	}
	if m.preview.source == PaneRemote {
		request.host = *m.remoteHost
		request.remoteID = m.remoteLoadID
		request.session = m.remoteSession
	}
	return request, true
}

func (m *Model) beginPreviewLoad(request previewRequest) tea.Cmd {
	if !m.preview.active || request.generation != m.preview.generation || request.source != m.preview.source || request.session != m.remoteSession {
		return nil
	}
	if request.source == PaneRemote && (m.remoteHost == nil || request.host.Name != m.remoteHost.Name) {
		return nil
	}
	if !request.force {
		current, ok := m.currentPreviewRequest(request.generation)
		if !ok || current.path != request.path {
			return nil
		}
	}

	m.preview.waiting = false
	if request.source == PaneRemote {
		if m.remoteBusy() {
			m.preview.waiting = true
			return nil
		}
		// A queued refresh can outlive the connection it was scheduled on.
		// Bind the read to the connection that will actually execute it.
		request.remoteID = m.remoteLoadID
		request.host = *m.remoteHost
		if m.remoteConn == nil || m.remoteConn.Err() != nil {
			return func() tea.Msg {
				return previewLoadFailure(request, errors.New("remote is not connected"))
			}
		}
		ctx, cancel := context.WithTimeout(context.Background(), previewTimeout)
		m.remotePreviewReading = true
		m.remotePreviewID = request.generation
		m.remotePreviewCancel = cancel
		m.remotePreviewConn = m.remoteConn
		return readRemotePreviewCmd(ctx, m.remoteConn, request)
	}
	return readLocalPreviewCmd(request)
}

func (m *Model) resumePreviewLoad() tea.Cmd {
	if !m.preview.active || !m.preview.waiting {
		return nil
	}
	return m.beginPreviewLoad(m.preview.pending)
}

// AcceptsPreviewResult reports whether a preview result is still current.
func (m Model) AcceptsPreviewResult(msg MsgPreviewLoaded) bool {
	if !m.preview.active || msg.request.generation != m.preview.generation || msg.request.source != m.preview.source || msg.request.session != m.remoteSession {
		return false
	}
	if !msg.Remote() {
		return true
	}
	return msg.request.session == m.remoteSession && msg.request.remoteID == m.remoteLoadID && m.remoteHost != nil && m.remoteHost.Name == msg.request.host.Name
}

func (m *Model) applyPreviewLoaded(msg msgPreviewLoaded) tea.Cmd {
	var imageCmd tea.Cmd
	accepted := m.AcceptsPreviewResult(msg)
	// Completing the dispatched read releases the connection even when its
	// content is obsolete or the preview has been closed. Older reads from a
	// different connection, browser, or operation must not release a newer one.
	ownedRemoteRead := msg.Remote() && m.remotePreviewReading &&
		msg.request.generation == m.remotePreviewID &&
		msg.request.session == m.remoteSession && msg.request.remoteID == m.remoteLoadID &&
		m.remoteHost != nil && msg.request.host.Name == m.remoteHost.Name
	if ownedRemoteRead {
		m.remotePreviewReading = false
		if m.remotePreviewCancel != nil {
			m.remotePreviewCancel()
			m.remotePreviewCancel = nil
		}
		m.remotePreviewConn = nil
	}

	if accepted {
		m.preview.loading = false
		m.preview.waiting = false
		if msg.err != nil {
			m.statusMsg = "Preview failed: " + sanitizePreviewError(msg.err)
			if !m.preview.loaded {
				m.preview.path = msg.request.path
				m.preview.message = previewFailureMessage(msg.err)
				m.preview.lines = nil
				m.preview.rows = nil
				m.preview.offset = 0
			}
		} else {
			imageCmd = m.dropPreviewImage()
			m.preview.loaded = true
			m.preview.path = msg.request.path
			if strings.HasPrefix(m.statusMsg, "Preview failed: ") {
				m.statusMsg = ""
			}
			m.preview.targetPath = msg.request.path
			m.preview.message = ""
			m.preview.lines = msg.lines
			m.preview.picture = msg.picture
			m.preview.rows = nil
			m.preview.offset = 0
			m.layoutPreview(false)
			imageCmd = tea.Batch(imageCmd, m.showPreviewImage())
		}
	}

	if ownedRemoteRead && (errors.Is(msg.err, context.Canceled) || errors.Is(msg.err, context.DeadlineExceeded)) {
		m.remoteConn = nil
		m.remoteLoadID++
		m.remoteStatus = "Remote preview timed out. Press [r] to reconnect."
		m.preview.waiting = false
		return imageCmd
	}
	return tea.Batch(imageCmd, m.resumePreviewLoad())
}

// cancelRemotePreview invalidates a dispatched remote read. The command's
// context watcher closes the connection to interrupt Open or Read, neither of
// which accepts a context.
func (m *Model) cancelRemotePreview(detachConnection bool) tea.Cmd {
	if m.remotePreviewCancel == nil {
		return nil
	}
	m.remotePreviewCancel()
	m.remotePreviewCancel = nil
	conn := m.remotePreviewConn
	m.remotePreviewConn = nil
	if m.remotePreviewReading {
		m.remotePreviewReading = false
		if detachConnection {
			m.remoteConn = nil
			m.remoteLoadID++
			m.remoteStatus = "Remote preview cancelled. Press [r] to reconnect."
		}
	}
	if !detachConnection || conn == nil {
		return nil
	}
	return func() tea.Msg {
		if err := conn.Close(); err != nil {
			log.Error("close cancelled preview connection", "err", err)
		}
		return nil
	}
}

func readLocalPreviewCmd(request previewRequest) tea.Cmd {
	return func() tea.Msg {
		info, err := os.Lstat(request.path)
		if err != nil {
			return previewLoadFailure(request, fmt.Errorf("stat %s: %w", request.path, err))
		}
		if !info.Mode().IsRegular() {
			return msgPreviewLoaded{request: request, err: errPreviewNotFile}
		}
		if !previewSizeAllowed(request, info.Size()) {
			return msgPreviewLoaded{request: request, err: errPreviewTooLarge}
		}

		file, err := os.Open(request.path)
		if err != nil {
			return previewLoadFailure(request, fmt.Errorf("open %s: %w", request.path, err))
		}
		lines, picture, readErr := readPreview(file, info.Size(), request)
		closeErr := file.Close()
		if err := errors.Join(readErr, closeErr); err != nil {
			if closeErr == nil && (errors.Is(readErr, errPreviewTooLarge) || errors.Is(readErr, errPreviewBinary)) {
				return msgPreviewLoaded{request: request, err: readErr}
			}
			return previewLoadFailure(request, fmt.Errorf("read %s: %w", request.path, err))
		}
		return msgPreviewLoaded{request: request, lines: lines, picture: picture}
	}
}

func readRemotePreviewCmd(ctx context.Context, conn remote.Client, request previewRequest) tea.Cmd {
	return func() tea.Msg {
		detach := remote.CloseOnContextDone(ctx, conn)
		defer detach()
		if !previewSizeAllowed(request, request.size) {
			return msgPreviewLoaded{request: request, err: errPreviewTooLarge}
		}
		reader, err := conn.Open(request.path)
		if err != nil {
			if ctx.Err() != nil {
				err = ctx.Err()
			}
			return previewLoadFailure(request, fmt.Errorf("open remote %s: %w", request.path, err))
		}
		lines, picture, readErr := readPreview(reader, request.size, request)
		closeErr := reader.Close()
		if err := errors.Join(readErr, closeErr); err != nil {
			if ctx.Err() != nil {
				err = ctx.Err()
			}
			if closeErr == nil && (errors.Is(readErr, errPreviewTooLarge) || errors.Is(readErr, errPreviewBinary)) {
				return msgPreviewLoaded{request: request, err: readErr}
			}
			return previewLoadFailure(request, fmt.Errorf("read remote %s: %w", request.path, err))
		}
		return msgPreviewLoaded{request: request, lines: lines, picture: picture}
	}
}

func previewLoadFailure(request previewRequest, err error) msgPreviewLoaded {
	location := "local"
	if request.source == PaneRemote {
		location = "remote"
	}
	log.Error("file preview failed", "location", location, "path", request.path, "err", err)
	return msgPreviewLoaded{request: request, err: err}
}

// previewSizeAllowed applies the text limit, or the larger image limit to
// files named like images when the terminal can show them.
func previewSizeAllowed(request previewRequest, size int64) bool {
	return size <= previewReadLimit(request)
}

func previewReadLimit(request previewRequest) int64 {
	if request.decodeImages && imagefile.HasImageExtension(request.path) {
		return imagefile.MaxBytes
	}
	return previewMaxBytes
}

// readPreview returns text lines or, for a recognized image, its picture.
func readPreview(reader io.Reader, knownSize int64, request previewRequest) ([]string, *previewPicture, error) {
	limit := previewReadLimit(request)
	if knownSize > limit {
		return nil, nil, errPreviewTooLarge
	}

	data, err := io.ReadAll(io.LimitReader(reader, limit+1))
	if err != nil {
		return nil, nil, err
	}
	if int64(len(data)) > limit {
		return nil, nil, errPreviewTooLarge
	}
	if info, ok := imagefile.Probe(data); ok {
		picture := &previewPicture{info: info}
		if request.decodeImages {
			picture.image, _, picture.err = imagefile.Decode(data, previewImageSide)
		}
		return nil, picture, nil
	}
	if len(data) > previewMaxBytes {
		return nil, nil, errPreviewTooLarge
	}
	lines, err := previewText(data)
	return lines, nil, err
}

func previewText(data []byte) ([]string, error) {
	if bytes.IndexByte(data, 0) >= 0 {
		return nil, errPreviewBinary
	}

	text := sanitizePreviewText(data)
	lines := strings.Split(text, "\n")
	if len(lines) > 1 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	if len(lines) == 0 {
		lines = []string{""}
	}
	return lines, nil
}

// previewImageArea is the cell area below the image's info line.
func (m Model) previewImageArea() (int, int) {
	leftWidth, rightWidth := m.paneWidths()
	width := rightWidth
	if m.preview.source == PaneRemote {
		width = leftWidth
	}
	return width - 2, m.viewportHeight() - 2
}

// showPreviewImage sends the previewed image to the terminal when it has not
// been placed for the current pane size yet.
func (m *Model) showPreviewImage() tea.Cmd {
	picture := m.preview.picture
	if !m.preview.active || picture == nil || picture.image == nil || !m.images.Enabled() {
		return nil
	}
	cols, rows := m.previewImageArea()
	if cols < 1 || rows < 1 || cols == picture.areaCols && rows == picture.areaRows {
		return nil
	}
	picture.areaCols, picture.areaRows = cols, rows
	picture.token++
	images, img, generation, token := m.images, picture.image, m.preview.generation, picture.token
	return func() tea.Msg {
		placement, err := images.Show(img, cols, rows)
		if err != nil {
			log.Error("show preview image", "err", err)
		}
		return msgPreviewImageShown{generation: generation, token: token, placement: placement, err: err}
	}
}

func (m *Model) applyPreviewImageShown(msg msgPreviewImageShown) tea.Cmd {
	picture := m.preview.picture
	if !m.preview.active || picture == nil || msg.generation != m.preview.generation || msg.token != picture.token {
		return m.deleteImage(msg.placement.ID)
	}
	if msg.err != nil {
		picture.err = msg.err
		return nil
	}
	previous := picture.placement.ID
	picture.placement = msg.placement
	picture.rows = termimage.Rows(msg.placement)
	return m.deleteImage(previous)
}

// ReleaseImages frees the previewed image in the terminal. Call it when the
// browser is replaced, for example when another project opens.
func (m *Model) ReleaseImages() tea.Cmd {
	return m.dropPreviewImage()
}

// dropPreviewImage forgets the previewed image and frees it in the terminal.
func (m *Model) dropPreviewImage() tea.Cmd {
	picture := m.preview.picture
	m.preview.picture = nil
	if picture == nil {
		return nil
	}
	// A placement still in flight is freed when its result arrives stale.
	picture.token++
	return m.deleteImage(picture.placement.ID)
}

func (m *Model) deleteImage(id uint32) tea.Cmd {
	if id == 0 {
		return nil
	}
	images := m.images
	return func() tea.Msg {
		if err := images.Delete(id); err != nil {
			log.Error("free terminal image", "err", err)
		}
		return nil
	}
}

func sanitizePreviewText(data []byte) string {
	text := strings.ToValidUTF8(string(data), "�")
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.ReplaceAll(text, "\r", "\n")

	var sanitized strings.Builder
	sanitized.Grow(len(text))
	for _, r := range text {
		switch r {
		case '\n':
			sanitized.WriteByte('\n')
		case '\t':
			sanitized.WriteString("    ")
		default:
			if !unicode.IsControl(r) {
				sanitized.WriteRune(r)
			}
		}
	}
	return sanitized.String()
}

func sanitizePreviewError(err error) string {
	message := sanitizePreviewText([]byte(err.Error()))
	return strings.Join(strings.Fields(message), " ")
}

func previewFailureMessage(err error) string {
	switch {
	case errors.Is(err, errPreviewTooLarge):
		return "Preview unavailable: file exceeds 1 MiB"
	case errors.Is(err, errPreviewBinary):
		return "Preview unavailable: binary file"
	case errors.Is(err, errPreviewNotFile):
		return "Preview unavailable: not a regular file"
	default:
		return "Preview unavailable: " + sanitizePreviewError(err)
	}
}

func (m *Model) layoutPreview(preserveTop bool) {
	if !m.preview.loaded {
		m.preview.rows = nil
		m.preview.numberWidth = 0
		m.preview.offset = 0
		return
	}

	topLine := 0
	if preserveTop && m.preview.offset >= 0 && m.preview.offset < len(m.preview.rows) {
		topLine = m.preview.rows[m.preview.offset].lineNumber
	}

	leftWidth, rightWidth := m.paneWidths()
	paneWidth := rightWidth
	if m.preview.source == PaneRemote {
		paneWidth = leftWidth
	}
	numberWidth := len(strconv.Itoa(len(m.preview.lines)))
	if maximum := paneWidth - 4; numberWidth > maximum {
		numberWidth = maximum
	}
	if numberWidth < 1 {
		numberWidth = 1
	}
	contentWidth := paneWidth - numberWidth - 3
	if contentWidth < 1 {
		contentWidth = 1
	}

	rows := make([]previewRow, 0, len(m.preview.lines))
	for index, line := range m.preview.lines {
		wrapped := strings.Split(ansi.Hardwrap(line, contentWidth, true), "\n")
		if len(wrapped) == 0 {
			wrapped = []string{""}
		}
		for part, text := range wrapped {
			rows = append(rows, previewRow{
				lineNumber:   index + 1,
				continuation: part > 0,
				text:         text,
			})
		}
	}

	m.preview.rows = rows
	m.preview.numberWidth = numberWidth
	m.preview.offset = 0
	if topLine > 0 {
		for index, row := range rows {
			if row.lineNumber == topLine {
				m.preview.offset = index
				break
			}
		}
	}
	m.clampPreviewScroll()
}

func (m *Model) clampPreviewScroll() {
	maximum := len(m.preview.rows) - m.viewportHeight()
	if maximum < 0 {
		maximum = 0
	}
	if m.preview.offset < 0 {
		m.preview.offset = 0
	}
	if m.preview.offset > maximum {
		m.preview.offset = maximum
	}
}

func (m *Model) scrollPreview(key string) {
	page := m.viewportHeight()
	switch key {
	case keyPgUp:
		m.preview.offset -= page
	case keyPgDown:
		m.preview.offset += page
	case keyHome:
		m.preview.offset = 0
	case keyEnd:
		m.preview.offset = len(m.preview.rows) - page
	}
	m.clampPreviewScroll()
}
