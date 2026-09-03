package hostmanager

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// Screen layout, top to bottom:
//
//	headerLines  title, separator
//	listHeight() entry rows
//	footerLines  separator, status
//
// hitTest in mouse.go maps clicks back through these, so a change here must be
// a change to the constants — not to a hand-counted number in View.
const (
	headerLines = 2
	footerLines = 2
)

// entryRowSpan returns how many screen rows an entry occupies. A section
// header takes two: a blank spacer above its label.
func entryRowSpan(e entry) int {
	if e.isHeader {
		return 2
	}
	return 1
}

// listHeight returns the number of entry rows the list can show.
func (m Model) listHeight() int {
	h := m.Height - headerLines - footerLines
	if h < 1 {
		return 1
	}
	return h
}

func (m Model) View() string {
	var sb strings.Builder

	// Header
	title := styles.Header.Render("drift") + "  " + styles.Muted.Render("Host Manager")
	sb.WriteString(padRight(title, m.Width))
	sb.WriteByte('\n')
	sb.WriteString(styles.Sep.Render(strings.Repeat("─", m.Width)))
	sb.WriteByte('\n')

	// Entry list
	vh := m.listHeight()
	rendered := 0
	for i, e := range m.entries {
		span := entryRowSpan(e)
		if rendered+span > vh {
			break
		}
		if e.isHeader {
			// A section header is a blank spacer row followed by its label.
			// Both are written here rather than smuggled in as a "\n" prefix,
			// so the row budget and hitTest see the same two rows the terminal
			// does.
			sb.WriteString(strings.Repeat(" ", m.Width))
			sb.WriteByte('\n')
			sb.WriteString(m.renderHeader(e.scope))
		} else {
			sb.WriteString(m.renderHost(e, i == m.cursor))
		}
		sb.WriteByte('\n')
		rendered += span
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
		// A project host is described by two files: the one above, which the
		// team shares, and the access store, which does not leave the machine.
		sub = withAccessNote(sub, m.Width)
	}
	line := "  " + styles.Key.Render(label) + "  " + styles.Muted.Render(sub)
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

// withAccessNote appends the reminder that a project host is described by two
// files: the project config, which the team shares, and the access store, which
// does not leave the machine. The host form names the exact path; here the point
// is only that the two are not the same file.
//
// It is dropped when the line would not fit, because a section header is
// exactly one row — the row budget and hitTest in mouse.go both count on that —
// so it must not wrap.
func withAccessNote(sub string, width int) string {
	const note = "  ·  access stays on this machine"
	if lipgloss.Width("  PROJECT HOSTS  "+sub+note) > width {
		return sub
	}
	return sub + note
}

func padRight(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}
