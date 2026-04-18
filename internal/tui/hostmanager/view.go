package hostmanager

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

func (m Model) View() string {
	var sb strings.Builder

	// Header
	title := styles.Header.Render("drift") + "  " + styles.Muted.Render("Host Manager")
	sb.WriteString(padRight(title, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')

	// Entry list
	vh := m.Height - 4
	if vh < 1 {
		vh = 1
	}
	rendered := 0
	for i, e := range m.entries {
		if rendered >= vh {
			break
		}
		if e.isHeader {
			sb.WriteString(m.renderHeader(e.scope))
		} else {
			sb.WriteString(m.renderHost(e, i == m.cursor))
		}
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

func (m Model) renderHeader(scope config.HostScope) string {
	var label, sub string
	switch scope {
	case config.ScopeGlobal:
		label = "GLOBAL HOSTS"
		sub = "~/.config/drift/config.toml"
	case config.ScopeProject:
		label = "PROJECT HOSTS"
		if m.cfg.ProjectRoot != "" {
			sub = m.cfg.ProjectRoot + "/.drift/config.toml"
		} else {
			sub = ".drift/config.toml (no project root found)"
		}
	}
	line := "\n  " + styles.Key.Render(label) + "  " + styles.Muted.Render(sub)
	return padRight(line, m.Width)
}

func (m Model) renderHost(e entry, isCursor bool) string {
	h := e.host

	authLabel := h.Auth.Type
	if authLabel == "" {
		authLabel = "keyfile"
	}

	port := ""
	if h.Port != 0 && h.Port != 22 {
		port = fmt.Sprintf(":%d", h.Port)
	}

	name := fmt.Sprintf("%-16s", h.Name)
	host := fmt.Sprintf("%-30s", h.Hostname+port)
	root := fmt.Sprintf("%-24s", h.RootPath)
	auth := fmt.Sprintf("%-10s", authLabel)

	line := "  " +
		styles.Dir.Render(name) + " " +
		styles.File.Render(host) + " " +
		styles.Muted.Render(root) + " " +
		styles.Muted.Render(auth)

	if lipgloss.Width(line) > m.Width {
		line = lipgloss.NewStyle().MaxWidth(m.Width).Render(line)
	}

	if isCursor {
		line = styles.CursorRow.Width(m.Width).Render(padRight(line, m.Width))
	} else {
		line = padRight(line, m.Width)
	}

	return line
}

func (m Model) renderStatus() string {
	if m.confirmDelete {
		e := m.currentEntry()
		if e != nil {
			msg := fmt.Sprintf("  Delete %q? ", e.host.Name)
			return styles.Warn.Render(msg) +
				styles.Key.Render("[y]") + styles.Muted.Render("es  ") +
				styles.Key.Render("[n]") + styles.Muted.Render("o")
		}
	}
	if m.testing {
		return styles.Warn.Render(fmt.Sprintf("  Testing %s…", m.testTarget))
	}
	if m.statusMsg != "" {
		if strings.HasPrefix(m.statusMsg, "✓") {
			return padRight(styles.File.Render("  "+m.statusMsg), m.Width)
		}
		return padRight(styles.Err.Render("  "+m.statusMsg), m.Width)
	}
	help := "  [n]new  [e]edit  [d]delete  [t]test  [Esc]back"
	return padRight(styles.Muted.Render(help), m.Width)
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}
