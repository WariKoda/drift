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
//	chromeRows()  file title, optional full local/remote paths, separator
//	viewport…     unified diff content
//
// hitTest in mouse.go maps clicks back through these, so a change here must be
// a change to the constants — not to a hand-counted number in View.
const (
	headerLines = 2
	footerLines = 2
	pathChrome  = 2 // file title + separator inside the right pane
	pathRows    = 2 // full local and remote paths, shown with [f]
)

// bodyTop is the first screen row of the side-by-side body.
const bodyTop = headerLines

type comparisonEmptyState uint8

const (
	comparisonNotEmpty comparisonEmptyState = iota
	comparisonNoDifferences
	comparisonNoFiles
)

// contentTop returns the first screen row occupied by the diff content
// (below the right-pane path chrome).
func (m Model) contentTop() int {
	return bodyTop + m.chromeRows()
}

func (m Model) chromeRows() int {
	if m.showPaths {
		return pathChrome + pathRows
	}
	return pathChrome
}

func (m Model) View() string {
	var sb strings.Builder

	s := m.activeSession()

	// ── Header ───────────────────────────────────────────────────────
	sb.WriteString(pad(m.renderHeader(), max(0, m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')

	// ── Body: comparison result or file list | diff ──────────────────
	if state := m.comparisonEmptyState(); state != comparisonNotEmpty && !m.showErrors && !m.showHelp {
		for _, row := range m.renderComparisonSplash(state) {
			sb.WriteString(row)
			sb.WriteByte('\n')
		}
	} else {
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
			if fw > 0 {
				sb.WriteString(dividerStyle.Render("│"))
			}
			sb.WriteString(r)
			sb.WriteByte('\n')
		}
	}

	// ── Bottom ────────────────────────────────────────────────────────
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')
	sb.WriteString(pad(m.renderStatus(s), max(0, m.Width)))

	return sb.String()
}

func (m Model) comparisonEmptyState() comparisonEmptyState {
	if len(m.sessions) == 0 {
		if m.scope.Pairs == 0 {
			return comparisonNoFiles
		}
		return comparisonNoDifferences
	}
	for i := range m.sessions {
		session := &m.sessions[i]
		if session.Err != nil || session.Result == nil || session.Result.HasDiff() {
			return comparisonNotEmpty
		}
	}
	return comparisonNoDifferences
}

func (m Model) renderComparisonSplash(state comparisonEmptyState) []string {
	title := "✓ No differences"
	detail := "Local and remote files are identical."
	count := m.scope.Pairs
	if count == 0 {
		count = len(m.sessions)
	}
	unit := "files"
	if count == 1 {
		unit = "file"
	}
	lines := []string{
		styles.Accent.Render(title),
		"",
		styles.File.Render(fmt.Sprintf("%d %s compared", count, unit)),
		styles.Muted.Render(detail),
	}
	if state == comparisonNoFiles {
		lines = []string{
			styles.Accent.Render("No files to compare"),
			"",
			styles.Muted.Render("The selected scope contains no comparable files."),
		}
	}

	height := m.bodyHeight()
	rows := blankRows(height, m.Width)
	start := max(0, (height-len(lines))/2)
	for i, line := range lines {
		if start+i >= height {
			break
		}
		line = ansi.Truncate(line, max(1, m.Width), "")
		left := max(0, (m.Width-lipgloss.Width(line))/2)
		rows[start+i] = pad(strings.Repeat(" ", left)+line, m.Width)
	}
	return rows
}

func (m Model) renderHeader() string {
	left := styles.Header.Render("drift")
	right := styles.Muted.Render("→ ") + styles.Dir.Render(styles.SafeText(m.host.Name)) +
		styles.Muted.Render("  "+styles.SafeText(m.host.Hostname))
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

	rows = append(rows, m.renderFileHeader(s, dw)...)
	planned := m.activeIdx >= 0 && m.activeIdx < len(m.syncDirs) && m.syncDirs[m.activeIdx] != DirNone

	vh := m.viewportHeight()
	var content []string
	switch {
	case m.showHelp:
		content = paddedContentRows([]string{"Diff help — [?], [Esc] or [q] to close", "", "[v] unified / split / auto", "[←/→] horizontal scroll", "[j/k], [PgUp/PgDn] vertical scroll", "[Tab/Shift+Tab] next / previous file", "[</>] narrower / wider file list", "[f] show / hide full local and remote paths", "File list: ~ changed, L local only, R remote only, B binary, ! error", "[[/]] previous / next hunk across files", "[Enter/l], [h], [c] fold controls", "[Space/A] action for current / all files", "[s/S] sync current / all", "[u/d] quick upload / download", "[i] include ignored, [r] refresh"}, vh, dw)
	case m.showErrors:
		content = m.renderErrorListRows(vh, dw)
	case s == nil:
		content = blankRows(vh, dw)
	case s.Err != nil:
		content = paddedContentRows([]string{"  " + styles.Err.Render(styles.SafeText(s.Err.Error()))}, vh, dw)
	case s.Result == nil:
		content = paddedContentRows([]string{"  " + styles.Muted.Render("loading…")}, vh, dw)
	case s.Result.Binary || len(s.Result.Lines) == 0:
		content = m.renderSummaryRows(s.Result, vh, dw)
	default:
		content = diff.RenderUnifiedRows(s.Result, m.displayRows(), dw, m.scroll, vh, diff.RenderOptions{Flip: m.remoteBeforeLocal(), Highlight: planned, Spans: m.spans, Split: m.splitLayout(), Offset: m.horizontal, Syntax: m.syntax})
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

// renderFileHeader returns chromeRows rows: the file title, the optional full
// paths in before/after order, and a separator.
func (m Model) renderFileHeader(s *diff.Session, width int) []string {
	rows := make([]string, 0, m.chromeRows())
	if s == nil {
		for len(rows) < m.chromeRows()-1 {
			rows = append(rows, strings.Repeat(" ", width))
		}
		return append(rows, sepLine(width))
	}
	rows = append(rows, m.renderFileTitle(s, width))
	if m.showPaths {
		labels, paths := [2]string{"Local ", "Remote"}, [2]string{s.LocalPath, s.RemotePath}
		if m.remoteBeforeLocal() {
			labels[0], labels[1], paths[0], paths[1] = labels[1], labels[0], paths[1], paths[0]
		}
		for i := range labels {
			value := styles.SafeText(paths[i])
			if value == "" {
				value = "—"
			}
			rows = append(rows, pad(styles.Muted.Render(labels[i]+" ")+styles.File.Render(truncLeft(value, max(1, width-7))), width))
		}
	}
	return append(rows, sepLine(width))
}

// renderFileTitle keeps the before/after orientation over the line counts and
// both over the path, which is shortened from the left.
func (m Model) renderFileTitle(s *diff.Session, width int) string {
	before, after := "Local", "Remote"
	if m.remoteBeforeLocal() {
		before, after = after, before
	}
	short := before + " → " + after
	long := short + " (before → after)"
	if m.activeIdx < 0 || m.activeIdx >= len(m.syncDirs) || m.syncDirs[m.activeIdx] == DirNone {
		long = "Compare only · " + short
	}
	title := styles.SafeText(m.displayPath(s))
	_, label, _ := fileStatus(s)
	if label != "" {
		title += " · " + label
	}
	counts := m.countsLabel(m.activeIdx)
	gap := ""
	if counts != "" {
		gap = "  "
	}
	right := ""
	for _, candidate := range []string{counts + gap + styles.Muted.Render(long), counts + gap + styles.Muted.Render(short), styles.Muted.Render(short)} {
		if width-lipgloss.Width(candidate)-1 >= min(ansi.StringWidth(title), 16) {
			right = candidate
			break
		}
	}
	left := truncLeft(title, max(1, width-lipgloss.Width(right)-1))
	space := max(1, width-ansi.StringWidth(left)-lipgloss.Width(right))
	return pad(styles.File.Render(left)+strings.Repeat(" ", space)+right, width)
}

// renderErrorListRows renders bulk-sync errors for the right pane.
func (m Model) renderErrorListRows(height, width int) []string {
	if height <= 0 {
		return nil
	}
	title := styles.Err.Render(fmt.Sprintf("Sync errors (%d)", len(m.syncErrors)))
	rows := []string{pad(ansi.Truncate("  "+title+styles.KeyHints("  — [e] or [q] to close", styles.Muted), width, ""), width)}
	content := m.errorRows(width)
	start := min(m.errorOffset, max(0, len(content)-max(1, height-1)))
	end := min(len(content), start+height-1)
	rows = append(rows, content[start:end]...)
	return paddedContentRows(rows, height, width)
}

func (m Model) errorRows(width int) []string {
	var rows []string
	for _, failure := range m.syncErrors {
		path, operation, reason := failure.Path, failure.Operation, failure.Reason
		if path == "" {
			path = "unknown file"
		}
		if operation == "" {
			operation = "sync"
		}
		if reason == "" {
			reason = "unknown error"
		}
		line := "✗ " + styles.SafeText(path) + " [" + styles.SafeText(operation) + "] " + styles.SafeText(reason)
		for _, part := range strings.Split(ansi.Hardwrap(line, max(1, width-2), true), "\n") {
			rows = append(rows, pad("  "+styles.Err.Render(part), width))
		}
	}
	return rows
}

func (m *Model) clampErrorScroll() {
	m.errorOffset = min(max(0, m.errorOffset), max(0, len(m.errorRows(m.diffWidth()))-max(1, m.viewportHeight()-1)))
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
	connectionErr := m.disconnected
	if connectionErr == nil && m.conn != nil {
		connectionErr = m.connectionError()
	}
	if connectionErr != nil {
		reason := strings.Join(strings.Fields(connectionErr.Error()), " ")
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
			added, removed := m.displayCounts(m.activeIdx)
			info = styles.File.Render(fmt.Sprintf("+%d -%d", added, removed))
		}
	}

	var keys string
	switch {
	case m.syncing:
		keys = styles.Warn.Render(m.syncProgressLabel())
	case m.quickSyncing:
		keys = styles.Warn.Render("syncing current file…")
	case m.showErrors:
		keys = styles.KeyHints("[e/q]close errors", styles.Muted)
	default:
		if m.comparisonEmptyState() != comparisonNotEmpty {
			keys = styles.KeyHints("[i]include ignored  [r]refresh  [q]back", styles.Muted)
		} else {
			keys = styles.KeyHints("[i]include ignored  [s/S]sync  [r]refresh  [q]back", styles.Muted)
		}
		keys = styles.KeyHints("[?]help [v]layout  ", styles.Muted) + keys
		if len(m.syncErrors) > 0 {
			keys = styles.KeyHints("[e]errors  ", styles.Err) + keys
		}
	}
	if m.decorationError != "" {
		info = styles.Err.Render(styles.SafeText(m.decorationError))
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
	width = max(0, width)
	w := lipgloss.Width(s)
	if w > width {
		return ansi.Truncate(s, width, "")
	}
	if w == width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

// truncLeft shortens p to max display columns, keeping the tail (the part that
// matters most for a path) and prefixing an ellipsis when truncated.
func truncLeft(p string, max int) string {
	if max < 1 {
		max = 1
	}
	if cells := ansi.StringWidth(p); cells > max {
		return ansi.TruncateLeft(p, cells-max+1, "…")
	}
	return p
}
