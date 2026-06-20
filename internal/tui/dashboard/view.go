package dashboard

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

func (m Model) View() string {
	var sb strings.Builder

	// Header
	title := styles.Header.Render("drift") + "  " + styles.Muted.Render("Projects")
	sb.WriteString(padRight(title, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')

	vh := m.Height - 4
	if vh < 1 {
		vh = 1
	}

	rendered := 0
	if len(m.entries) == 0 {
		sb.WriteString(padRight(styles.Muted.Render("  No projects yet — press [n] to add one."), m.Width))
		sb.WriteByte('\n')
		rendered++
	}
	for i, e := range m.entries {
		if rendered >= vh {
			break
		}
		sb.WriteString(m.renderProject(e, i == m.cursor))
		sb.WriteByte('\n')
		rendered++
	}
	for rendered < vh {
		sb.WriteString(strings.Repeat(" ", m.Width))
		sb.WriteByte('\n')
		rendered++
	}

	// Bottom separator + status/help
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(m.renderStatus())

	return sb.String()
}

func (m Model) renderProject(e entry, isCursor bool) string {
	name := fmt.Sprintf("%-20s", truncate(e.proj.Name, 20))
	path := fmt.Sprintf("%-40s", truncate(collapseHome(e.proj.Path), 40))

	var badge string
	switch {
	case e.missing:
		badge = styles.Err.Render("missing")
	case e.proj.Archived:
		badge = styles.Warn.Render("archived")
	}

	line := "  " +
		styles.Dir.Render(name) + " " +
		styles.Muted.Render(path) + " " +
		badge

	if lipgloss.Width(line) > m.Width {
		line = lipgloss.NewStyle().MaxWidth(m.Width).Render(line)
	}

	if isCursor {
		return styles.CursorRow.Width(m.Width).Render(padRight(line, m.Width))
	}
	return padRight(line, m.Width)
}

func (m Model) renderStatus() string {
	if m.confirmDelete {
		if e := m.currentEntry(); e != nil {
			msg := fmt.Sprintf("  Remove %q from the registry? ", e.proj.Name)
			return styles.Warn.Render(msg) +
				styles.Key.Render("[y]") + styles.Muted.Render("es  ") +
				styles.Key.Render("[n]") + styles.Muted.Render("o")
		}
	}
	if m.statusMsg != "" {
		return padRight(styles.Err.Render("  "+m.statusMsg), m.Width)
	}
	archived := "[.]show archived"
	if m.showArchived {
		archived = "[.]hide archived"
	}
	help := "  [↵]open  [n]new  [e]edit  [d]remove  [a]archive  " + archived + "  [q]quit"
	return padRight(styles.Muted.Render(help), m.Width)
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

func truncate(s string, max int) string {
	r := []rune(s)
	if len(r) <= max {
		return s
	}
	if max <= 1 {
		return string(r[:max])
	}
	return string(r[:max-1]) + "…"
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}
