package projectform

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

func (m Model) View() string {
	var sb strings.Builder

	title := "New Project"
	if m.isEdit {
		title = "Edit Project: " + m.oldSlug
	}
	header := styles.Header.Render("drift") + "  " + styles.Muted.Render(title)
	sb.WriteString(padRight(header, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	sb.WriteString("  " + styles.Muted.Render("Name — display name, e.g. KUNDE A"))
	sb.WriteByte('\n')
	sb.WriteString("  " + styles.Muted.Render("Path — local project directory (hosts live in <path>/.drift/config.toml)"))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	for _, f := range m.fields {
		if f != nil {
			sb.WriteString(f.View())
			sb.WriteByte('\n')
		}
	}

	sb.WriteByte('\n')
	if m.errMsg != "" {
		sb.WriteString("  " + styles.Err.Render("✗ "+m.errMsg))
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(styles.Muted.Render("  [Tab/↓]next  [Shift+Tab/↑]prev  [Ctrl+S / Enter on last]save  [Esc]cancel"))

	return sb.String()
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}
