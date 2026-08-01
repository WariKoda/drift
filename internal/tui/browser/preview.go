package browser

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
	"unicode"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

const (
	previewMaxBytes = 1 << 20
	previewDebounce = 150 * time.Millisecond
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
}

type msgPreviewDebounced struct {
	request previewRequest
}

type msgPreviewLoaded struct {
	request previewRequest
	lines   []string
	err     error
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
	rows        []previewRow
	numberWidth int
	offset      int
	pending     previewRequest
}

func (m *Model) togglePreview() tea.Cmd {
	if m.preview.active {
		m.disablePreview()
		return nil
	}

	generation := m.preview.generation + 1
	m.preview = filePreview{
		active:     true,
		source:     m.activePane,
		generation: generation,
		message:    "Select a regular file to preview",
	}
	return m.schedulePreview()
}

func (m *Model) disablePreview() {
	generation := m.preview.generation + 1
	m.preview = filePreview{generation: generation}
	if strings.HasPrefix(m.statusMsg, "Preview failed: ") {
		m.statusMsg = ""
	}
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
	return previewRequest{
		generation: generation,
		source:     m.preview.source,
		path:       filePath,
		size:       entry.Size,
	}, true
}

func (m *Model) beginPreviewLoad(request previewRequest) tea.Cmd {
	if !m.preview.active || request.generation != m.preview.generation || request.source != m.preview.source {
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
		if m.remoteConn == nil {
			return func() tea.Msg {
				return previewLoadFailure(request, errors.New("remote is not connected"))
			}
		}
		m.remotePreviewReading = true
		return readRemotePreviewCmd(m.remoteConn, request)
	}
	return readLocalPreviewCmd(request)
}

func (m *Model) resumePreviewLoad() tea.Cmd {
	if !m.preview.active || !m.preview.waiting {
		return nil
	}
	return m.beginPreviewLoad(m.preview.pending)
}

func (m *Model) applyPreviewLoaded(msg msgPreviewLoaded) tea.Cmd {
	if msg.request.source == PaneRemote {
		m.remotePreviewReading = false
	}

	if m.preview.active && msg.request.generation == m.preview.generation && msg.request.source == m.preview.source {
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
			m.preview.loaded = true
			m.preview.path = msg.request.path
			if strings.HasPrefix(m.statusMsg, "Preview failed: ") {
				m.statusMsg = ""
			}
			m.preview.targetPath = msg.request.path
			m.preview.message = ""
			m.preview.lines = msg.lines
			m.preview.rows = nil
			m.preview.offset = 0
			m.layoutPreview(false)
		}
	}

	return m.resumePreviewLoad()
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
		if info.Size() > previewMaxBytes {
			return msgPreviewLoaded{request: request, err: errPreviewTooLarge}
		}

		file, err := os.Open(request.path)
		if err != nil {
			return previewLoadFailure(request, fmt.Errorf("open %s: %w", request.path, err))
		}
		lines, readErr := readPreviewText(file, info.Size())
		closeErr := file.Close()
		if err := errors.Join(readErr, closeErr); err != nil {
			if closeErr == nil && (errors.Is(readErr, errPreviewTooLarge) || errors.Is(readErr, errPreviewBinary)) {
				return msgPreviewLoaded{request: request, err: readErr}
			}
			return previewLoadFailure(request, fmt.Errorf("read %s: %w", request.path, err))
		}
		return msgPreviewLoaded{request: request, lines: lines}
	}
}

func readRemotePreviewCmd(conn remote.Client, request previewRequest) tea.Cmd {
	return func() tea.Msg {
		if request.size > previewMaxBytes {
			return msgPreviewLoaded{request: request, err: errPreviewTooLarge}
		}
		reader, err := conn.Open(request.path)
		if err != nil {
			return previewLoadFailure(request, fmt.Errorf("open remote %s: %w", request.path, err))
		}
		lines, readErr := readPreviewText(reader, request.size)
		closeErr := reader.Close()
		if err := errors.Join(readErr, closeErr); err != nil {
			if closeErr == nil && (errors.Is(readErr, errPreviewTooLarge) || errors.Is(readErr, errPreviewBinary)) {
				return msgPreviewLoaded{request: request, err: readErr}
			}
			return previewLoadFailure(request, fmt.Errorf("read remote %s: %w", request.path, err))
		}
		return msgPreviewLoaded{request: request, lines: lines}
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

func readPreviewText(reader io.Reader, knownSize int64) ([]string, error) {
	if knownSize > previewMaxBytes {
		return nil, errPreviewTooLarge
	}

	data, err := io.ReadAll(io.LimitReader(reader, previewMaxBytes+1))
	if err != nil {
		return nil, err
	}
	if len(data) > previewMaxBytes {
		return nil, errPreviewTooLarge
	}
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
