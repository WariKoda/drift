package diffview

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/nibra180/drift/internal/diff"
	"github.com/nibra180/drift/internal/styles"
)

var dividerStyle = lipgloss.NewStyle().Foreground(styles.ColorSep)

func (m Model) View() string {
	var sb strings.Builder

	s := m.activeSession()

	// ── Header ───────────────────────────────────────────────────────
	sb.WriteString(m.renderHeader())
	sb.WriteByte('\n')
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')

	// ── File list ─────────────────────────────────────────────────────
	sb.WriteString(m.renderFileList())
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')

	// ── Column labels ─────────────────────────────────────────────────
	pw := m.paneWidth()
	localLabel := pad(styles.Key.Render("  LOCAL"), pw)
	remoteLabel := pad(styles.Key.Render("  REMOTE  ")+styles.Muted.Render(m.host.Hostname), pw)
	sb.WriteString(localLabel + dividerStyle.Render("│") + remoteLabel)
	sb.WriteByte('\n')
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')

	// ── Diff content ──────────────────────────────────────────────────
	vh := m.viewportHeight()

	if s == nil {
		for i := 0; i < vh; i++ {
			sb.WriteString(strings.Repeat(" ", m.Width))
			sb.WriteByte('\n')
		}
	} else if s.Err != nil {
		sb.WriteString("  " + styles.Err.Render(s.Err.Error()))
		sb.WriteByte('\n')
		for i := 1; i < vh; i++ {
			sb.WriteString(strings.Repeat(" ", m.Width))
			sb.WriteByte('\n')
		}
	} else if s.Result == nil {
		sb.WriteString("  " + styles.Muted.Render("loading…"))
		sb.WriteByte('\n')
		for i := 1; i < vh; i++ {
			sb.WriteString(strings.Repeat(" ", m.Width))
			sb.WriteByte('\n')
		}
	} else if s.Result.Binary || len(s.Result.Lines) == 0 {
		sb.WriteString(m.renderSummary(s.Result, vh))
	} else {
		leftLines, rightLines := diff.RenderPanes(s.Result, pw, m.scroll, vh)
		for i := 0; i < vh; i++ {
			l := ""
			r := ""
			if i < len(leftLines) {
				l = leftLines[i]
			} else {
				l = strings.Repeat(" ", pw)
			}
			if i < len(rightLines) {
				r = rightLines[i]
			} else {
				r = strings.Repeat(" ", pw)
			}
			sb.WriteString(l)
			sb.WriteString(dividerStyle.Render("│"))
			sb.WriteString(r)
			sb.WriteByte('\n')
		}
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

func (m Model) renderFileList() string {
	var sb strings.Builder
	pw := m.paneWidth()
	fh := m.fileListHeight()
	end := m.fileListOffset + fh
	if end > len(m.sessions) {
		end = len(m.sessions)
	}

	for i := m.fileListOffset; i < end; i++ {
		s := &m.sessions[i]
		active := i == m.activeIdx

		// cursor (2 visible chars: indicator + space)
		var cursor string
		if active {
			cursor = styles.Key.Render("▶") + " "
		} else {
			cursor = "  "
		}

		// direction arrow (center, replaces │ divider)
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

		// name style: active = File, inactive = Muted
		var nameStyle lipgloss.Style
		if active {
			nameStyle = styles.File
		} else {
			nameStyle = styles.Muted
		}

		// Determine which sides have a file.
		localOnly := s.Result != nil && s.Result.LocalOnly
		remoteOnly := s.Result != nil && s.Result.RemoteOnly

		// ── Left pane: cursor + local name (empty when RemoteOnly) ────
		var leftPane string
		if remoteOnly {
			leftPane = pad(cursor, pw)
		} else {
			localName := shortPath(s.LocalPath)
			localMaxW := pw - 2
			if localMaxW < 2 {
				localMaxW = 2
			}
			if r := []rune(localName); len(r) > localMaxW {
				localName = "…" + string(r[len(r)-(localMaxW-1):])
			}
			leftPane = pad(cursor+nameStyle.Render(localName), pw)
		}

		// ── Right pane: remote name (empty when LocalOnly) ────────────
		var rightPane string
		if localOnly {
			rightPane = pad("", pw)
		} else {
			remoteName := shortPath(s.RemotePath)
			remoteMaxW := pw - 1
			if remoteMaxW < 2 {
				remoteMaxW = 2
			}
			if r := []rune(remoteName); len(r) > remoteMaxW {
				remoteName = "…" + string(r[len(r)-(remoteMaxW-1):])
			}
			rightPane = pad(" "+nameStyle.Render(remoteName), pw)
		}

		sb.WriteString(leftPane + dirChar + rightPane + "\n")
	}

	return sb.String()
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
		added, removed := countDiff(r)
		return styles.File.Render(fmt.Sprintf("+%d -%d", added, removed))
	}
}

func (m Model) renderSummary(r *diff.DiffResult, height int) string {
	var lines []string
	switch {
	case r.Binary:
		lines = append(lines,
			"  "+styles.Muted.Render("Binary file — cannot show text diff"),
			fmt.Sprintf("  local:  %s  (%d bytes)", r.ModLocal.Format("2006-01-02 15:04"), r.SizeLocal),
			fmt.Sprintf("  remote: %s  (%d bytes)", r.ModRemote.Format("2006-01-02 15:04"), r.SizeRemote),
		)
	default:
		lines = append(lines, "  "+styles.Muted.Render("Files are identical"))
	}

	var sb strings.Builder
	for _, l := range lines {
		sb.WriteString(l)
		sb.WriteByte('\n')
	}
	for i := len(lines); i < height; i++ {
		sb.WriteString(strings.Repeat(" ", m.Width))
		sb.WriteByte('\n')
	}
	return sb.String()
}

func (m Model) renderStatus(s *diff.Session) string {
	var info string
	if s != nil && s.Result != nil {
		if !s.Result.HasDiff() {
			info = styles.Muted.Render("identical")
		} else {
			added, removed := countDiff(s.Result)
			info = styles.File.Render(fmt.Sprintf("+%d -%d", added, removed))
		}
	}

	var keys string
	switch {
	case m.syncing:
		keys = styles.Warn.Render("syncing…")
	case m.refreshing:
		keys = styles.Warn.Render("refreshing…")
	default:
		keys = styles.Muted.Render("[j/k]file  [Space]dir  [A]dir-all  [s]sync  [S]sync-all  [r]refresh  [u/d]quick  [q]back")
	}
	if m.syncStatus != "" && !m.syncing && !m.refreshing {
		info = styles.File.Render(m.syncStatus)
	}
	gap := m.Width - lipgloss.Width(info) - lipgloss.Width(keys) - 2
	if gap < 1 {
		gap = 1
	}
	return "  " + info + strings.Repeat(" ", gap) + keys
}

func countDiff(r *diff.DiffResult) (added, removed int) {
	for _, l := range r.Lines {
		switch l.Kind {
		case diff.LineAdded:
			added++
		case diff.LineRemoved:
			removed++
		}
	}
	return
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
