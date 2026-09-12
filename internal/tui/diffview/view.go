package diffview

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

var dividerStyle = lipgloss.NewStyle().Foreground(styles.ColorSep)

// Screen layout:
//
//	headerLines   header, separator (full width)
//	bodyHeight()  left file list │ right path chrome + unified diff
//	footerLines   separator, status (full width)
//
// Right pane within the body (top to bottom):
//
//	pathChrome    path header, line-number legend, separator
//	viewport…     unified diff content
//
// hitTest in mouse.go maps clicks back through these, so a change here must be
// a change to the constants — not to a hand-counted number in View.
const (
	headerLines = 2
	footerLines = 2
	pathChrome  = 3 // path header + line-number legend + separator inside the right pane
)

// bodyTop is the first screen row of the side-by-side body.
const bodyTop = headerLines

// contentTop returns the first screen row occupied by the diff content
// (below the right-pane path chrome).
func (m Model) contentTop() int {
	return bodyTop + pathChrome
}

func (m Model) View() string {
	var sb strings.Builder

	s := m.activeSession()

	// ── Header ───────────────────────────────────────────────────────
	sb.WriteString(m.renderHeader())
	sb.WriteByte('\n')
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')

	// ── Body: file list | diff ───────────────────────────────────────
	fw := m.fileListWidth()
	dw := m.diffWidth()
	left := m.renderFileListRows()
	right := m.renderDiffPaneRows(s)
	bh := m.bodyHeight()
	for i := 0; i < bh; i++ {
		l, r := "", ""
		if i < len(left) {
			l = left[i]
		} else {
			l = strings.Repeat(" ", fw)
		}
		if i < len(right) {
			r = right[i]
		} else {
			r = strings.Repeat(" ", dw)
		}
		sb.WriteString(l)
		sb.WriteString(dividerStyle.Render("│"))
		sb.WriteString(r)
		sb.WriteByte('\n')
	}

	// ── Bottom ────────────────────────────────────────────────────────
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')
	sb.WriteString(m.renderStatus(s))

	return sb.String()
}

func (m Model) renderHeader() string {
	left := styles.Header.Render("drift")
	right := styles.Muted.Render("→ ") + styles.Dir.Render(m.host.Name) +
		styles.Muted.Render("  "+m.host.Hostname)
	gap := m.Width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
	}
	return left + strings.Repeat(" ", gap) + right
}

// renderDiffPaneRows builds the right pane: path chrome, then content rows.
func (m Model) renderDiffPaneRows(s *diff.Session) []string {
	dw := m.diffWidth()
	bh := m.bodyHeight()
	rows := make([]string, 0, bh)

	localPath, remotePath := "", ""
	if s != nil {
		localPath, remotePath = s.LocalPath, s.RemotePath
	}
	rows = append(rows, m.renderPathHeader(localPath, remotePath, dw))
	before, after := "Local", "Remote"
	if m.remoteBeforeLocal() {
		before, after = after, before
	}
	legend := fmt.Sprintf("Line numbers: %s → %s (before → after)", before, after)
	rows = append(rows, pad(styles.Muted.Render(ansi.Truncate(legend, dw, "")), dw))
	rows = append(rows, sepLine(dw))

	vh := m.viewportHeight()
	var content []string
	switch {
	case m.showErrors:
		content = m.renderErrorListRows(vh, dw)
	case s == nil:
		content = blankRows(vh, dw)
	case s.Err != nil:
		content = paddedContentRows([]string{"  " + styles.Err.Render(s.Err.Error())}, vh, dw)
	case s.Result == nil:
		content = paddedContentRows([]string{"  " + styles.Muted.Render("loading…")}, vh, dw)
	case s.Result.Binary || len(s.Result.Lines) == 0:
		content = m.renderSummaryRows(s.Result, vh, dw)
	default:
		content = diff.RenderUnifiedRows(s.Result, m.displayRows(), dw, m.scroll, vh, m.remoteBeforeLocal())
		for len(content) < vh {
			content = append(content, strings.Repeat(" ", dw))
		}
	}
	rows = append(rows, content...)
	for len(rows) < bh {
		rows = append(rows, strings.Repeat(" ", dw))
	}
	return rows
}

// renderPathHeader follows the same before/after order as the line-number columns.
func (m Model) renderPathHeader(localPath, remotePath string, width int) string {
	sep := " │ "
	half := (width - len(sep)) / 2
	if half < 1 {
		half = 1
	}
	if m.remoteBeforeLocal() {
		localPath, remotePath = remotePath, localPath
	}
	left := truncLeft(localPath, half)
	right := truncLeft(remotePath, half)
	return pad(styles.Muted.Render(left+sep+right), width)
}

// renderFileListRows returns bodyHeight padded rows for the left sidebar.
func (m Model) renderFileListRows() []string {
	fw := m.fileListWidth()
	fh := m.fileListHeight()
	rows := make([]string, 0, fh)

	end := m.fileListOffset + fh
	if end > len(m.sessions) {
		end = len(m.sessions)
	}

	for i := m.fileListOffset; i < end; i++ {
		rows = append(rows, m.renderFileRow(i, fw))
	}
	for len(rows) < fh {
		rows = append(rows, strings.Repeat(" ", fw))
	}
	return rows
}

func (m Model) renderFileRow(i, width int) string {
	s := &m.sessions[i]
	active := i == m.activeIdx

	var cursor string
	if active {
		cursor = styles.Key.Render("▶") + " "
	} else {
		cursor = "  "
	}

	var dirChar string
	dir := DirNone
	if i < len(m.syncDirs) {
		dir = m.syncDirs[i]
	}
	switch dir {
	case DirUpload:
		dirChar = styles.Marked.Render("↑")
	case DirDownload:
		dirChar = styles.Dir.Render("↓")
	case DirDeleteLocal, DirDeleteRemote:
		dirChar = styles.Err.Render("✗")
	default:
		dirChar = styles.Muted.Render("—")
	}

	if m.completed[i] && dir == DirNone {
		dirChar = styles.Marked.Render("✓")
	}

	var nameStyle lipgloss.Style
	if active {
		nameStyle = styles.File
	} else {
		nameStyle = styles.Muted
	}

	name := fileListName(s)
	// cursor(2) + name + space + dir(1)
	nameMax := width - 4
	if nameMax < 2 {
		nameMax = 2
	}
	if r := []rune(name); len(r) > nameMax {
		name = "…" + string(r[len(r)-(nameMax-1):])
	}

	return pad(cursor+nameStyle.Render(name)+" "+dirChar, width)
}

// fileListName picks the path shown in the narrow sidebar.
func fileListName(s *diff.Session) string {
	if s.Result != nil && s.Result.RemoteOnly {
		return shortPath(s.RemotePath)
	}
	if s.LocalPath != "" {
		return shortPath(s.LocalPath)
	}
	return shortPath(s.RemotePath)
}

// sessionStatus returns the diff status string for a session.
func sessionStatus(s *diff.Session) string {
	if s.Err != nil {
		return styles.Err.Render("error")
	}
	if s.Result == nil {
		return styles.Muted.Render("loading…")
	}
	r := s.Result
	switch {
	case r.LocalOnly:
		return styles.Warn.Render("local only")
	case r.RemoteOnly:
		return styles.Warn.Render("remote only")
	case r.Binary:
		return styles.Muted.Render("binary")
	case !r.HasDiff():
		return styles.Muted.Render("identical")
	default:
		added, removed := r.Counts()
		return styles.File.Render(fmt.Sprintf("+%d -%d", added, removed))
	}
}

// renderErrorListRows renders bulk-sync errors for the right pane.
func (m Model) renderErrorListRows(height, width int) []string {
	rows := make([]string, 0, height)

	title := styles.Err.Render(fmt.Sprintf("Sync errors (%d)", len(m.syncErrors)))
	rows = append(rows, pad("  "+title+styles.KeyHints("  — [e] or [q] to close", styles.Muted), width))

	for _, failure := range m.syncErrors {
		if len(rows) >= height {
			break
		}
		contentWidth := max(1, width-4)
		path := failure.Path
		if path == "" {
			path = "unknown file"
		}
		path = truncLeft(path, max(1, contentWidth/3))
		operation := failure.Operation
		if operation == "" {
			operation = "sync"
		}
		prefix := "✗ " + path + " [" + operation + "] "
		reason := failure.Reason
		if reason == "" {
			reason = "unknown error"
		}
		// Keep the leading outcome warning visible even when its cause is long.
		reason = ansi.Truncate(reason, max(1, contentWidth-lipgloss.Width(prefix)), "…")
		rows = append(rows, pad("  "+styles.Err.Render("✗ ")+styles.File.Render(path)+
			styles.Muted.Render(" ["+operation+"] ")+styles.Err.Render(reason), width))
	}

	return paddedContentRows(rows, height, width)
}

func (m Model) renderSummaryRows(r *diff.DiffResult, height, width int) []string {
	var lines []string
	switch {
	case r.LocalOnly:
		lines = []string{"  " + styles.Warn.Render("local only")}
	case r.RemoteOnly:
		lines = []string{"  " + styles.Warn.Render("remote only")}
	case r.Binary:
		lines = []string{"  " + styles.Muted.Render("Binary file — cannot show text diff")}
	default:
		lines = []string{"  " + styles.Muted.Render("Files are identical")}
	}
	if r.LocalOnly || r.RemoteOnly || r.Binary {
		local := "  local:  not present"
		if !r.RemoteOnly {
			local = fmt.Sprintf("  local:  %s  (%d bytes)", r.ModLocal.Format("2006-01-02 15:04"), r.SizeLocal)
		}
		remote := "  remote: not present"
		if !r.LocalOnly {
			remote = fmt.Sprintf("  remote: %s  (%d bytes)", r.ModRemote.Format("2006-01-02 15:04"), r.SizeRemote)
		}
		lines = append(lines, local, remote)
	}
	return paddedContentRows(lines, height, width)
}

func paddedContentRows(lines []string, height, width int) []string {
	rows := make([]string, 0, height)
	for _, l := range lines {
		if len(rows) >= height {
			break
		}
		rows = append(rows, pad(l, width))
	}
	for len(rows) < height {
		rows = append(rows, strings.Repeat(" ", width))
	}
	return rows
}

func blankRows(height, width int) []string {
	rows := make([]string, height)
	blank := strings.Repeat(" ", width)
	for i := range rows {
		rows[i] = blank
	}
	return rows
}

func (m Model) renderStatus(s *diff.Session) string {
	if m.disconnected != nil || (m.conn != nil && m.conn.Err() != nil) {
		reason := strings.Join(strings.Fields(m.connectionError().Error()), " ")
		status := "Disconnected: " + reason + " | Reopen comparison before syncing."
		if len(m.syncErrors) > 0 {
			status += " [e]errors"
		}
		status += " [q]back"
		return ansi.Truncate(styles.KeyHints(status, styles.Err), max(1, m.Width), "…")
	}
	var info string
	if s != nil && s.Result != nil {
		if !s.Result.HasDiff() {
			info = styles.Muted.Render("identical")
		} else if s.Result.ContentDiff {
			info = styles.File.Render("content differs")
		} else {
			added, removed := s.Result.Counts()
			info = styles.File.Render(fmt.Sprintf("+%d -%d", added, removed))
		}
	}

	var keys string
	switch {
	case m.syncing:
		keys = styles.Warn.Render(m.syncProgressLabel())
	case m.quickSyncing:
		keys = styles.Warn.Render("syncing current file…")
	case m.refreshing:
		keys = styles.Warn.Render("refreshing…")
	case m.showErrors:
		keys = styles.KeyHints("[e/q]close errors", styles.Muted)
	default:
		if m.scopeSet {
			keys = styles.KeyHints("[i]include ignored  [s/S]sync  [r]refresh  [q]back", styles.Muted)
		} else {
			keys = styles.KeyHints("[Tab]file  [j/k/Pg/g/G]scroll  [[]/[]]hunk  [Enter]fold  [Space]dir  [s/S]sync  [r]refresh  [u/d]quick  [q]back", styles.Muted)
		}
		if len(m.syncErrors) > 0 {
			keys = styles.KeyHints("[e]errors  ", styles.Err) + keys
		}
	}
	if m.syncStatus != "" && !m.remoteBusy() {
		info = styles.KeyHints(m.syncStatus, styles.File)
	} else if scope := m.scopeSummaryLabel(); scope != "" {
		if info != "" {
			info += styles.Muted.Render(" · ")
		}
		info += styles.Muted.Render(scope)
	}
	availableInfo := m.Width - lipgloss.Width(keys) - 3
	if availableInfo < 1 {
		availableInfo = 1
	}
	info = ansi.Truncate(info, availableInfo, "…")
	gap := m.Width - lipgloss.Width(info) - lipgloss.Width(keys) - 2
	if gap < 1 {
		gap = 1
	}
	return "  " + info + strings.Repeat(" ", gap) + keys
}

func (m Model) scopeSummaryLabel() string {
	if !m.scopeSet {
		return ""
	}
	parts := []string{fmt.Sprintf("%d pairs", m.scope.Pairs), fmt.Sprintf("%d hidden", m.scope.Hidden)}
	if m.scope.IgnoredFilesSkipped > 0 {
		parts = append(parts, fmt.Sprintf("%d ignored skipped", m.scope.IgnoredFilesSkipped))
	}
	if m.scope.IgnoredDirsSkipped > 0 {
		parts = append(parts, fmt.Sprintf("%d ignored dirs skipped", m.scope.IgnoredDirsSkipped))
	}
	if m.scope.HardExcludedSkipped > 0 {
		parts = append(parts, fmt.Sprintf("%d fixed excluded", m.scope.HardExcludedSkipped))
	}
	if m.scope.ExplicitIgnoredIncluded > 0 {
		parts = append(parts, fmt.Sprintf("%d explicit ignored included", m.scope.ExplicitIgnoredIncluded))
	}
	state := "off"
	if m.scopeOptions.IncludeIgnored {
		state = "on"
	}
	parts = append(parts, "include ignored: "+state)
	return strings.Join(parts, " · ")
}

// syncProgressLabel renders the live bulk-sync progress as a small bar with a
// file counter, e.g. "syncing [████░░░░] 4/10".
func (m Model) syncProgressLabel() string {
	if m.syncTotal <= 0 {
		return "syncing…"
	}
	const barWidth = 10
	filled := m.syncDone * barWidth / m.syncTotal
	if filled > barWidth {
		filled = barWidth
	}
	bar := strings.Repeat("█", filled) + strings.Repeat("░", barWidth-filled)
	return fmt.Sprintf("syncing [%s] %d/%d", bar, m.syncDone, m.syncTotal)
}

func sepLine(width int) string {
	return styles.Sep.Render(strings.Repeat("─", width))
}

func pad(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

func shortPath(p string) string {
	parts := strings.Split(p, "/")
	if len(parts) > 2 {
		return "…/" + strings.Join(parts[len(parts)-2:], "/")
	}
	return p
}

// truncLeft shortens p to max display columns, keeping the tail (the part that
// matters most for a path) and prefixing an ellipsis when truncated.
func truncLeft(p string, max int) string {
	if max < 1 {
		max = 1
	}
	r := []rune(p)
	if len(r) <= max {
		return p
	}
	if max == 1 {
		return "…"
	}
	return "…" + string(r[len(r)-(max-1):])
}
