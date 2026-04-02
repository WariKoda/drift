package browser

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/nibra180/drift/internal/fs"
	"github.com/nibra180/drift/internal/styles"
)

// View renders the browser screen.
func (m Model) View() string {
	if m.showHelp {
		return m.renderHelp()
	}

	var sb strings.Builder

	sb.WriteString(m.renderHeader())
	sb.WriteByte('\n')
	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')

	entries := m.filteredEntries()
	vh := m.viewportHeight()
	start := m.offset
	end := start + vh
	if end > len(entries) {
		end = len(entries)
	}

	rendered := 0
	for i := start; i < end; i++ {
		sb.WriteString(m.renderEntry(entries, i, i == m.cursor))
		sb.WriteByte('\n')
		rendered++
	}
	for rendered < vh {
		sb.WriteString(strings.Repeat(" ", m.Width))
		sb.WriteByte('\n')
		rendered++
	}

	sb.WriteString(m.renderSep())
	sb.WriteByte('\n')
	sb.WriteString(m.renderStatus(entries))

	return sb.String()
}

func (m Model) renderHeader() string {
	line := styles.Header.Render("drift") + "  " + styles.Muted.Render(m.absWorkDir())
	return padRight(line, m.Width)
}

func (m Model) renderSep() string {
	return styles.Sep.Render(strings.Repeat("─", m.Width))
}

func (m Model) renderEntry(entries []*fs.FileEntry, i int, isCursor bool) string {
	entry := entries[i]

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

	mark := "  "
	if m.Selection.IsMarked(entry.Path) {
		mark = styles.Marked.Render("● ")
	}

	var name string
	if m.filter != "" {
		name = highlightMatch(entry.Name, m.filter, entry.Kind)
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

	// Truncate long lines
	maxW := m.Width - 1
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
		line = styles.CursorRow.Width(m.Width).Render(padRight(line, m.Width))
	} else {
		line = padRight(line, m.Width)
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
	case m.Selection.Count() > 0:
		left = styles.Marked.Render(fmt.Sprintf("%d marked", m.Selection.Count())) +
			"  " + styles.Muted.Render(HelpText())
	default:
		left = styles.Muted.Render(fmt.Sprintf("%d items", len(entries))) +
			"  " + styles.Muted.Render(HelpText())
	}
	return padRight(left, m.Width)
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

// keep truncate from being flagged as unused by the linter
var _ = truncate
