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
	} else if m.isDuplicate {
		title = "Duplicate Host"
	} else if m.linked != nil {
		title = "Link Server: " + m.linked.Server
	}
	header := styles.Header.Render("drift") + "  " + styles.Muted.Render(title)
	sb.WriteString(padRight(header, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	if m.linked != nil {
		sb.WriteString(m.renderServer())
	}

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
			if m.projectSlug == "" {
				scopeLabels[1] = "project (none open)"
			}
			sb.WriteString(m.renderToggle("Scope", scopeLabels, int(m.scope), isFocused))
		case fMappings:
			sb.WriteString(m.renderMappingsRow(isFocused))
		case fKeepAliveDisabled:
			selected := 0
			if m.keepAliveDisabled {
				selected = 1
			}
			sb.WriteString(m.renderToggle("Disable keep-alive", []string{"no", "yes"}, selected, isFocused))
		case fKeepAliveInterval:
			sb.WriteString(m.fields[rowIdx].View())
			sb.WriteByte('\n')
			sb.WriteString(styles.Muted.Render(fmt.Sprintf("  Blank uses %.0f seconds.", config.DefaultKeepAliveInterval.Seconds())))
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
	if m.offer != nil {
		sb.WriteString("  " + styles.Warn.Render("Same server as "+m.offerLabel+". Link it instead of storing the connection twice?"))
		sb.WriteByte('\n')
		sb.WriteString(styles.KeyHints("  [l]ink, keeping name, root path and mappings  [s]ave as its own connection  [any]back", styles.Muted))
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	hints := "  [Tab/↓]next  [Shift+Tab/↑]prev  [Ctrl+S / Enter on last]save  [Esc]cancel"
	if m.linked != nil {
		hints = "  [Tab/↓]next  [Shift+Tab/↑]prev  [Ctrl+S]save  [Esc]cancel"
	}
	sb.WriteString(styles.KeyHints(hints, styles.Muted))

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
		sb.WriteString("  " + styles.Err.Render(`Delete "`+m.mappings[m.mapCursor].Local+`"?`) + styles.KeyHints("  [y]yes  [any]cancel", styles.Err))
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(styles.KeyHints("  [n]new  [e/Enter]edit  [d]delete  [Esc]back to form", styles.Muted))

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
	sb.WriteString("  " + styles.Muted.Render("Deploy Path — relative to the host Root Path"))
	sb.WriteByte('\n')
	sb.WriteByte('\n')

	for _, f := range m.editFields {
		if f != nil {
			sb.WriteString(f.View())
			sb.WriteByte('\n')
		}
	}

	if m.errMsg != "" {
		sb.WriteByte('\n')
		sb.WriteString("  " + styles.Err.Render(m.errMsg))
		sb.WriteByte('\n')
	}

	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')
	sb.WriteString(styles.KeyHints("  [Tab/↓]next  [Ctrl+S / Enter on last]save  [Esc]cancel", styles.Muted))

	return sb.String()
}

// renderServer shows the connection a link takes from its server. It is not a
// row: nothing here is edited in this form.
func (m Model) renderServer() string {
	h := m.linked
	endpoint := h.Hostname
	if h.User != "" {
		endpoint = h.User + "@" + endpoint
	}
	if h.Port != 0 && h.Port != config.DefaultPort(h.Protocol) {
		endpoint += fmt.Sprintf(":%d", h.Port)
	}
	protocol := h.Protocol
	if protocol == "" {
		protocol = "sftp"
	}
	line := "  " + styles.Muted.Render(padStr("Server", 14)) + " " +
		styles.Badge.Render(h.Server) + "  " + styles.File.Render(endpoint) + "  " + styles.Muted.Render(protocol)
	note := "  " + styles.Muted.Render("Connection and credentials belong to the global server; edit them there.")
	return line + "\n" + note + "\n\n"
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
		hint = styles.KeyHints("  [Enter] edit", styles.Muted)
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
		hint = styles.Dir.Render("  ← →")
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
