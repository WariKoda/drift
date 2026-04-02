package hostform

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/nibra180/drift/internal/config"
	"github.com/nibra180/drift/internal/styles"
)

func (m Model) View() string {
	var sb strings.Builder

	title := "New Host"
	if m.isEdit {
		title = "Edit Host: " + m.oldName
	}
	header := styles.Header.Render("drift") + "  " + styles.Muted.Render(title)
	sb.WriteString(padRight(header, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	rows := m.visibleRows()
	for ri, rowIdx := range rows {
		isFocused := ri == m.focusRow

		switch rowIdx {
		case fProtocol:
			sb.WriteString(m.renderToggle("Protocol", []string{"sftp", "ftp"}, int(m.protocol), isFocused))
		case fAuthType:
			sb.WriteString(m.renderToggle("Auth Type", []string{"keyfile", "password", "agent"}, int(m.authType), isFocused))
		case fScope:
			scopeLabels := []string{"global", "project"}
			if m.projectRoot == "" {
				scopeLabels[1] = "project (no .drift found)"
			}
			sb.WriteString(m.renderToggle("Scope", scopeLabels, int(m.scope), isFocused))
		default:
			if rowIdx < len(m.fields) && m.fields[rowIdx] != nil {
				sb.WriteString(m.fields[rowIdx].View())
			}
		}
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')

	if m.errMsg != "" {
		sb.WriteString("  " + styles.Err.Render("✗ "+m.errMsg))
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(styles.Muted.Render("  [Tab/↓]next field  [Shift+Tab/↑]prev  [Ctrl+S / Enter on last]save  [Esc]cancel"))

	return sb.String()
}

func (m Model) renderToggle(label string, options []string, selected int, focused bool) string {
	var labelStyle lipgloss.Style
	if focused {
		labelStyle = styles.File
	} else {
		labelStyle = styles.Muted
	}
	l := labelStyle.Render(padStr(label, 14))

	var parts []string
	for i, opt := range options {
		if i == selected {
			parts = append(parts, styles.Badge.Render(opt))
		} else {
			parts = append(parts, styles.Muted.Render(opt))
		}
	}

	hint := ""
	if focused {
		hint = styles.Muted.Render("  ← →")
	}

	return "  " + l + " " + strings.Join(parts, "  ") + hint
}

// scopeLabel returns display text for the project scope option.
func scopeLabel(projectRoot string) string {
	if projectRoot == "" {
		return "project (no .drift found)"
	}
	return "project"
}

// keep scopeLabel reachable (used indirectly via view logic above)
var _ = scopeLabel

func padStr(s string, n int) string {
	r := []rune(s)
	if len(r) >= n {
		return s
	}
	return s + strings.Repeat(" ", n-len(r))
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

// ensure config import is used (scope type comparison)
var _ config.HostScope
