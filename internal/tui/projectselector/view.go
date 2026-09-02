package projectselector

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

const (
	boxCols  = 54
	nameCols = 16
)

func (m Model) View() string {
	boxWidth := boxCols
	if m.Width > 0 && m.Width < boxWidth+4 {
		boxWidth = m.Width - 4
		if boxWidth < 20 {
			boxWidth = 20
		}
	}
	innerW := boxWidth - 2

	border := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(styles.ColorDir).
		Padding(0, 1).
		Width(boxWidth)

	var inner strings.Builder

	inner.WriteString(styles.Header.Render("Switch project"))
	inner.WriteByte('\n')

	query := m.query + "█"
	if m.query == "" {
		query = styles.Muted.Render("type to filter…")
	}
	inner.WriteString(styles.Sep.Render("  > ") + query)
	inner.WriteByte('\n')
	inner.WriteString(styles.Sep.Render(strings.Repeat("─", innerW)))
	inner.WriteByte('\n')

	for _, row := range m.listRows(m.maxListRows(), innerW) {
		inner.WriteString(row)
		inner.WriteByte('\n')
	}

	inner.WriteString("\n  " +
		styles.Key.Render("[Enter]") + styles.Muted.Render("open  ") +
		styles.Key.Render("[m]") + styles.Muted.Render("manage  ") +
		styles.Key.Render("[Esc]") + styles.Muted.Render("back"))

	if m.statusMsg != "" {
		inner.WriteByte('\n')
		inner.WriteString(styles.Err.Render(m.statusMsg))
	}

	return border.Render(inner.String())
}

func (m Model) maxListRows() int {
	h := m.Height
	if h < 1 {
		h = 24
	}
	// border (2) + title + filter + sep + blank + footer
	chrome := 7
	if m.statusMsg != "" {
		chrome++
	}
	n := h - chrome
	if n < 1 {
		return 1
	}
	return n
}

func (m Model) listRows(maxRows, inner int) []string {
	if len(m.filtered) == 0 {
		return []string{styles.Muted.Render("  no projects found")}
	}

	n := len(m.filtered)
	start := 0
	if n > maxRows {
		start = m.cursor - maxRows/2
		if start < 0 {
			start = 0
		}
		if start > n-maxRows {
			start = n - maxRows
		}
	}
	end := start + maxRows
	if end > n {
		end = n
	}

	rows := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		rows = append(rows, m.renderRow(i, inner))
	}
	return rows
}

func (m Model) renderRow(i, inner int) string {
	e := m.filtered[i]
	active := i == m.cursor
	isCurrent := m.currentSlug != "" && e.proj.Slug == m.currentSlug

	marker := "◇ "
	if active {
		marker = "◆ "
	}
	var st lipgloss.Style
	switch {
	case e.missing:
		st = styles.Err
	case active:
		st = styles.Accent
	default:
		st = styles.File
	}

	tag := ""
	tagWidth := 0
	if isCurrent {
		tag = " " + styles.Muted.Render("current")
		tagWidth = 1 + len("current")
	}

	pathWidth := inner - 2 - nameCols - 1 - tagWidth
	if pathWidth < 8 {
		pathWidth = 8
	}

	line := st.Render(marker) +
		st.Render(padRune(e.proj.Name, nameCols)) +
		" " +
		styles.Muted.Render(padRuneLeft(collapseHome(e.proj.Path), pathWidth)) +
		tag
	if active {
		line = styles.CursorRow.Width(inner).Render(padRight(line, inner))
	}
	return line
}

// collapseHome rewrites a path under $HOME to use ~ for display.
func collapseHome(p string) string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return p
	}
	if p == home {
		return "~"
	}
	if rel, err := filepath.Rel(home, p); err == nil && !strings.HasPrefix(rel, "..") {
		return "~" + string(filepath.Separator) + rel
	}
	return p
}

func padRune(s string, width int) string {
	r := []rune(s)
	if len(r) > width {
		if width <= 1 {
			return string(r[:width])
		}
		return string(r[:width-1]) + "…"
	}
	return s + strings.Repeat(" ", width-len(r))
}

func padRuneLeft(s string, width int) string {
	r := []rune(s)
	if len(r) > width {
		if width <= 1 {
			return string(r[len(r)-width:])
		}
		return "…" + string(r[len(r)-(width-1):])
	}
	return s + strings.Repeat(" ", width-len(r))
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}
