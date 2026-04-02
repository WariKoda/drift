package diffview

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/yourusername/drift/internal/diff"
	"github.com/yourusername/drift/internal/styles"
)

var dividerStyle = lipgloss.NewStyle().Foreground(styles.ColorSep)

func (m Model) View() string {
	var sb strings.Builder

	s := m.activeSession()

	// ── Header ───────────────────────────────────────────────────────
	sb.WriteString(m.renderHeader(s))
	sb.WriteByte('\n')
	sb.WriteString(sepLine(m.Width))
	sb.WriteByte('\n')

	// ── Column labels ─────────────────────────────────────────────────
	pw := m.paneWidth()
	localLabel := pad(styles.Key.Render("  LOCAL"), pw)
	remoteLabel := pad(styles.Key.Render("  REMOTE  ") + styles.Muted.Render(m.host.Hostname), pw)
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
	} else if s.Result.LocalOnly || s.Result.RemoteOnly || s.Result.Binary || len(s.Result.Lines) == 0 {
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

func (m Model) renderHeader(s *diff.Session) string {
	var filename string
	if s != nil {
		filename = shortPath(s.LocalPath)
	}
	nav := fmt.Sprintf("[%d/%d]", m.activeIdx+1, len(m.sessions))

	left := styles.Header.Render("drift") + "  " +
		styles.File.Render(filename) + "  " +
		styles.Muted.Render(nav)

	right := styles.Muted.Render("→ ") + styles.Dir.Render(m.host.Name) +
		styles.Muted.Render("  "+m.host.Hostname)

	gap := m.Width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
	}
	return left + strings.Repeat(" ", gap) + right
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
	case r.LocalOnly:
		lines = append(lines,
			"  "+styles.Warn.Render("File only exists locally"),
			"  "+styles.Muted.Render("Press [u] to upload"),
		)
	case r.RemoteOnly:
		lines = append(lines,
			"  "+styles.Warn.Render("File only exists on remote"),
			"  "+styles.Muted.Render("Press [d] to download"),
		)
	case len(r.Lines) == 0:
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
		total := len(s.Result.Lines)
		hasDiff := s.Result.HasDiff()
		if !hasDiff {
			info = styles.Muted.Render("identical")
		} else {
			added, removed := countDiff(s.Result)
			info = styles.File.Render(fmt.Sprintf("+%d -%d", added, removed))
		}
		_ = total
	}

	keys := styles.Muted.Render("[u]upload  [d]download  [Tab]next  [Shift+Tab]prev  []/[ ]hunks  [q]back")
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
	// show last 2 path segments
	parts := strings.Split(p, "/")
	if len(parts) > 2 {
		return "…/" + strings.Join(parts[len(parts)-2:], "/")
	}
	return p
}
