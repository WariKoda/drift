package browser

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// View renders the browser screen.
func (m Model) View() string {
	if m.finder.active {
		return m.renderFinder()
	}
	if m.showHelp {
		return m.renderHelp()
	}

	var sb strings.Builder
	localEntries := m.filteredEntries()
	leftW, rightW := m.paneWidths()
	divider := styles.Sep.Render("│")

	sb.WriteString(m.renderHeader())
	sb.WriteByte('\n')
	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')
	sb.WriteString(m.renderPaneLabels(leftW, rightW))
	sb.WriteByte('\n')
	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')

	vh := m.viewportHeight()
	for row := 0; row < vh; row++ {
		left := m.renderLocalRow(localEntries, m.offset+row, leftW)
		right := m.renderRemoteRow(m.remoteOffset+row, rightW)
		sb.WriteString(left)
		sb.WriteString(divider)
		sb.WriteString(right)
		sb.WriteByte('\n')
	}

	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')
	sb.WriteString(m.renderStatus(localEntries))

	return sb.String()
}

func (m Model) paneWidths() (int, int) {
	left := (m.Width - 1) / 2
	if left < 10 {
		left = 10
	}
	right := m.Width - left - 1
	if right < 10 {
		right = 10
	}
	return left, right
}

func (m Model) renderHeader() string {
	line := styles.Header.Render("drift") + "  " + styles.Muted.Render(m.absWorkDir())
	return padRight(line, m.Width)
}

func (m Model) renderPaneLabels(leftW, rightW int) string {
	localLabel := styles.Key.Render("  LOCAL  ") + styles.Muted.Render(truncLeftPath(m.absWorkDir(), leftW-10))
	if m.activePane != PaneLocal {
		localLabel = styles.Muted.Render("  LOCAL  ") + styles.Muted.Render(truncLeftPath(m.absWorkDir(), leftW-10))
	}

	remotePath := "[@] select host"
	remoteName := "REMOTE"
	if m.remoteHost != nil {
		remoteName = "REMOTE " + m.remoteHost.Name
		remotePath = m.remoteRoot
	}
	remoteHead := styles.Key.Render("  " + remoteName + "  ")
	if m.activePane != PaneRemote {
		remoteHead = styles.Muted.Render("  " + remoteName + "  ")
	}
	remoteLabel := remoteHead + styles.Muted.Render(truncLeftPath(remotePath, rightW-lipgloss.Width(remoteName)-4))

	return padRight(truncate(localLabel, leftW), leftW) + styles.Sep.Render("│") + padRight(truncate(remoteLabel, rightW), rightW)
}

func (m Model) renderSep() string {
	return styles.Sep.Render(strings.Repeat("─", m.Width))
}

func (m Model) renderLocalRow(entries []*fs.FileEntry, i, width int) string {
	if i < 0 || i >= len(entries) {
		return strings.Repeat(" ", width)
	}
	return m.renderEntry(entries[i], i == m.cursor && m.activePane == PaneLocal, width, m.Selection, m.filter)
}

func (m Model) renderRemoteRow(i, width int) string {
	switch {
	case m.remoteHost == nil:
		if i == 0 {
			return padRight("  "+styles.Muted.Render("press [@] to choose remote host"), width)
		}
		return strings.Repeat(" ", width)
	case m.remoteLoading:
		if i == 0 {
			return padRight("  "+styles.Muted.Render(m.remoteStatus), width)
		}
		return strings.Repeat(" ", width)
	case len(m.remoteEntries) == 0:
		if i == 0 {
			return padRight("  "+styles.Muted.Render("empty"), width)
		}
		return strings.Repeat(" ", width)
	case i < 0 || i >= len(m.remoteEntries):
		return strings.Repeat(" ", width)
	default:
		return m.renderEntry(m.remoteEntries[i], i == m.remoteCursor && m.activePane == PaneRemote, width, m.RemoteSelection, "")
	}
}

func (m Model) renderEntry(entry *fs.FileEntry, isCursor bool, width int, selection *fs.SelectionState, filter string) string {
	indent := strings.Repeat("  ", entry.Depth)

	var icon string
	if entry.Kind == fs.EntryDir {
		if entry.Expanded {
			icon = styles.Muted.Render("▼ ")
		} else {
			icon = styles.Muted.Render("▶ ")
		}
	} else {
		icon = "  "
	}

	mark := ""
	if selection != nil {
		mark = "  "
		if selection.IsMarked(entry.Path) {
			mark = styles.Marked.Render("● ")
		}
	}

	var name string
	if filter != "" {
		name = highlightMatch(entry.Name, filter, entry.Kind)
	} else {
		switch entry.Kind {
		case fs.EntryDir:
			name = styles.Dir.Render(entry.Name + "/")
		case fs.EntrySymlink:
			name = styles.Link.Render(entry.Name + "@")
		default:
			name = styles.File.Render(entry.Name)
		}
	}

	line := indent + icon + mark + name
	maxW := width - 1
	if lipgloss.Width(line) > maxW {
		prefix := indent + icon + mark
		available := maxW - lipgloss.Width(prefix) - 1
		if available < 1 {
			available = 1
		}
		short := truncatePlain(entry.Name, available) + "…"
		switch entry.Kind {
		case fs.EntryDir:
			name = styles.Dir.Render(short)
		case fs.EntrySymlink:
			name = styles.Link.Render(short)
		default:
			name = styles.File.Render(short)
		}
		line = prefix + name
	}

	if isCursor {
		line = styles.CursorRow.Width(width).Render(padRight(line, width))
	} else {
		line = padRight(line, width)
	}
	return line
}

func (m Model) renderStatus(entries []*fs.FileEntry) string {
	var left string
	switch {
	case m.filterMode:
		left = styles.Key.Render("/") + " " + m.filter + "█"
	case m.statusMsg != "":
		left = styles.Muted.Render(m.statusMsg)
	case selectionCount(m.Selection)+selectionCount(m.RemoteSelection) > 0:
		left = styles.Marked.Render(markedStatus(selectionCount(m.Selection), selectionCount(m.RemoteSelection)))
		if m.remoteStatus != "" {
			left += "  " + styles.Muted.Render(m.remoteStatus)
		}
	default:
		left = styles.Muted.Render(fmt.Sprintf("%d local items", len(entries)))
		if m.remoteStatus != "" {
			left += "  " + styles.Muted.Render(m.remoteStatus)
		}
	}

	right := styles.Muted.Render(HelpText())
	gap := m.Width - lipgloss.Width(left) - lipgloss.Width(right) - 2
	if gap < 1 {
		gap = 1
	}
	line := "  " + left + strings.Repeat(" ", gap) + right
	return padRight(truncate(line, m.Width), m.Width)
}

func markedStatus(local, remote int) string {
	switch {
	case local > 0 && remote > 0:
		return fmt.Sprintf("%d marked (%d local, %d remote)", local+remote, local, remote)
	case remote > 0:
		return fmt.Sprintf("%d remote marked", remote)
	default:
		return fmt.Sprintf("%d marked", local)
	}
}

func selectionCount(sel *fs.SelectionState) int {
	if sel == nil {
		return 0
	}
	return sel.Count()
}

func (m Model) renderHelp() string {
	var sb strings.Builder
	sb.WriteString(m.renderHeader())
	sb.WriteByte('\n')
	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')
	sb.WriteString(styles.File.Render(FullHelp()))
	sb.WriteByte('\n')
	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')
	sb.WriteString(styles.Muted.Render("  [?] close help"))
	return sb.String()
}

// renderFinder draws the fuzzy file finder overlay.
func (m Model) renderFinder() string {
	var sb strings.Builder

	title := styles.Header.Render("drift") + "  " + styles.Muted.Render("Find files — "+m.absWorkDir())
	sb.WriteString(padRight(title, m.Width))
	sb.WriteByte('\n')

	prompt := "  " + styles.Key.Render("›") + " " + m.finder.query + "█"
	sb.WriteString(padRight(prompt, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')

	vh := m.finderViewportHeight()
	rendered := 0
	switch {
	case m.finder.loading:
		sb.WriteString(padRight("  "+styles.Muted.Render("indexing project…"), m.Width))
		sb.WriteByte('\n')
		rendered++
	case len(m.finder.results) == 0:
		sb.WriteString(padRight("  "+styles.Muted.Render("no matches"), m.Width))
		sb.WriteByte('\n')
		rendered++
	default:
		start := m.finder.offset
		end := start + vh
		if end > len(m.finder.results) {
			end = len(m.finder.results)
		}
		for i := start; i < end; i++ {
			sb.WriteString(m.renderFinderRow(i))
			sb.WriteByte('\n')
			rendered++
		}
	}
	for rendered < vh {
		sb.WriteString(strings.Repeat(" ", m.Width))
		sb.WriteByte('\n')
		rendered++
	}

	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')
	help := fmt.Sprintf("  %d/%d  ·  [↑↓] move  [Space] mark  [Enter] done  [Esc] cancel  ·  %d marked",
		len(m.finder.results), len(m.finder.rel), m.Selection.Count())
	sb.WriteString(padRight(styles.Muted.Render(help), m.Width))
	return sb.String()
}

func (m Model) renderFinderRow(i int) string {
	r := m.finder.results[i]
	active := i == m.finder.cursor

	cursor := "  "
	if active {
		cursor = styles.Key.Render("▶ ")
	}
	mark := "  "
	if m.Selection.IsMarked(r.abs) {
		mark = styles.Marked.Render("● ")
	}

	// Filename first (scannable), directory after as dimmed context so that
	// look-alike names are easy to tell apart.
	dir := filepath.Dir(r.rel)
	baseStart := 0
	if dir != "." {
		baseStart = len([]rune(dir)) + 1 // skip "dir/"
	}
	baseStyle := styles.Muted
	if active {
		baseStyle = styles.File
	}
	line := cursor + mark + highlightRunes(r.rel, r.matched, baseStart, baseStyle)

	if dir != "." {
		remaining := (m.Width - 1) - lipgloss.Width(line) - 2
		if remaining >= 6 {
			line += "  " + styles.Sep.Render(truncLeftPath(dir, remaining))
		}
	}

	if lipgloss.Width(line) > m.Width-1 {
		line = lipgloss.NewStyle().MaxWidth(m.Width - 1).Render(line)
	}
	if active {
		return styles.CursorRow.Width(m.Width).Render(padRight(line, m.Width))
	}
	return padRight(line, m.Width)
}

// highlightRunes renders runes of s from start onward, emphasising the rune
// positions in matched (absolute indexes into s).
func highlightRunes(s string, matched []int, start int, base lipgloss.Style) string {
	runes := []rune(s)
	if start < 0 {
		start = 0
	}
	set := make(map[int]struct{}, len(matched))
	for _, idx := range matched {
		set[idx] = struct{}{}
	}
	hl := lipgloss.NewStyle().Foreground(styles.ColorMatch).Bold(true)
	var b strings.Builder
	for i := start; i < len(runes); i++ {
		if _, ok := set[i]; ok {
			b.WriteString(hl.Render(string(runes[i])))
		} else {
			b.WriteString(base.Render(string(runes[i])))
		}
	}
	return b.String()
}

// truncLeftPath shortens a path to max columns, keeping the tail (the immediate
// parent is the most useful for disambiguation).
func truncLeftPath(s string, max int) string {
	r := []rune(s)
	if len(r) <= max {
		return s
	}
	if max <= 1 {
		return "…"
	}
	return "…" + string(r[len(r)-(max-1):])
}

func highlightMatch(name, filter string, kind fs.EntryKind) string {
	lower := toLower(name)
	idx := indexStr(lower, toLower(filter))

	var baseStyle lipgloss.Style
	suffix := ""
	switch kind {
	case fs.EntryDir:
		baseStyle = styles.Dir
		suffix = "/"
	case fs.EntrySymlink:
		baseStyle = styles.Link
		suffix = "@"
	default:
		baseStyle = styles.File
	}

	if idx < 0 {
		return baseStyle.Render(name + suffix)
	}

	before := name[:idx]
	match := name[idx : idx+len(filter)]
	after := name[idx+len(filter):]

	hl := lipgloss.NewStyle().
		Foreground(styles.ColorMatch).
		Underline(true).
		Render(match)

	return baseStyle.Render(before) + hl + baseStyle.Render(after+suffix)
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

func truncate(s string, width int) string {
	if lipgloss.Width(s) <= width {
		return s
	}
	return lipgloss.NewStyle().MaxWidth(width).Render(s)
}

func truncatePlain(s string, n int) string {
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	return string(r[:n])
}
