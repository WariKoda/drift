package hostform

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

func (m Model) View() string {
	switch m.sub {
	case subMappingList:
		return m.viewMappingList()
	case subMappingEdit:
		return m.viewMappingEdit()
	default:
		return m.viewMain()
	}
}

func (m Model) viewMain() string {
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
			sb.WriteString(m.renderToggle("Protocol", []string{"sftp", "ftp", "ftps"}, int(m.protocol), isFocused))
		case fAuthType:
			sb.WriteString(m.renderToggle("Auth Type", []string{"keyfile", "password", "agent"}, int(m.authType), isFocused))
		case fScope:
			scopeLabels := []string{"global", "project"}
			if m.projectRoot == "" {
				scopeLabels[1] = "project (no .drift found)"
			}
			sb.WriteString(m.renderToggle("Scope", scopeLabels, int(m.scope), isFocused))
		case fMappings:
			sb.WriteString(m.renderMappingsRow(isFocused))
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
	sb.WriteString(styles.Muted.Render("  [Tab/↓]next  [Shift+Tab/↑]prev  [Ctrl+S / Enter on last]save  [Esc]cancel"))

	return sb.String()
}

func (m Model) viewMappingList() string {
	var sb strings.Builder

	hostName := m.fields[fName].Value()
	if hostName == "" {
		hostName = "new host"
	}
	header := styles.Header.Render("drift") + "  " + styles.Muted.Render("Mappings — "+hostName)
	sb.WriteString(padRight(header, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	if len(m.mappings) == 0 {
		sb.WriteString("  " + styles.Muted.Render("No mappings configured."))
		sb.WriteByte('\n')
		sb.WriteString("  " + styles.Muted.Render("Without mappings all files sync relative to Root Path."))
		sb.WriteByte('\n')
	} else {
		for i, mp := range m.mappings {
			cursor := "  "
			localStyle := styles.Muted
			remoteStyle := styles.Muted
			if i == m.mapCursor {
				cursor = styles.Marked.Render("▶ ")
				localStyle = styles.File
				remoteStyle = styles.Dir
			}
			line := cursor + localStyle.Render(mp.Local) + "  →  " + remoteStyle.Render(mp.Remote)
			sb.WriteString(line)
			sb.WriteByte('\n')
		}
	}

	sb.WriteByte('\n')

	if m.mapConfirmDel && m.mapCursor < len(m.mappings) {
		sb.WriteString("  " + styles.Err.Render(`Delete "`+m.mappings[m.mapCursor].Local+`"?  [y]yes  [any]cancel`))
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(styles.Muted.Render("  [n]new  [e/Enter]edit  [d]delete  [Esc]back to form"))

	return sb.String()
}

func (m Model) viewMappingEdit() string {
	var sb strings.Builder

	title := "New Mapping"
	if m.editIdx >= 0 {
		title = "Edit Mapping"
	}
	header := styles.Header.Render("drift") + "  " + styles.Muted.Render(title)
	sb.WriteString(padRight(header, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	sb.WriteString("  " + styles.Muted.Render("Local Path  — relative to project root (e.g. plugins/plugin1)"))
	sb.WriteByte('\n')
	sb.WriteString("  " + styles.Muted.Render("Deploy Path — absolute path on the server"))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	for _, f := range m.editFields {
		if f != nil {
			sb.WriteString(f.View())
			sb.WriteByte('\n')
		}
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(styles.Muted.Render("  [Tab/↓]next  [Ctrl+S / Enter on last]save  [Esc]cancel"))

	return sb.String()
}

func (m Model) renderMappingsRow(focused bool) string {
	var labelStyle lipgloss.Style
	if focused {
		labelStyle = styles.File
	} else {
		labelStyle = styles.Muted
	}
	label := labelStyle.Render(padStr("Mappings", 14))

	n := len(m.mappings)
	var val string
	switch n {
	case 0:
		val = "none"
	case 1:
		val = "1 mapping"
	default:
		val = fmt.Sprintf("%d mappings", n)
	}

	hint := ""
	if focused {
		hint = styles.Muted.Render("  [Enter] edit")
	}

	return "  " + label + " " + styles.Badge.Render(val) + hint
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
