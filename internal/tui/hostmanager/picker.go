package hostmanager

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/styles"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// MsgLinkPickerRequested asks the root model for the hosts the open project
// can link to. Reading them means reading every project store, which is the
// root's business, not the screen's.
type MsgLinkPickerRequested struct{}

// MsgLinkTargetChosen is sent when the user picks a host to link. A target of
// another project has been confirmed for promotion to a global server.
type MsgLinkTargetChosen struct {
	Target config.LinkTarget
}

// pickRow is a picker line: a group header or a target.
type pickRow struct {
	header string // group label; empty for a target row
	target config.LinkTarget
	usedBy string // project names linking a global server, joined
}

// OpenPicker shows targets grouped by owner. names maps project slugs to the
// names shown to the user; a slug without one is shown as is.
func (m *Model) OpenPicker(targets []config.LinkTarget, names map[string]string) {
	m.choosingNew = false
	if len(targets) == 0 {
		m.statusMsg = "Nothing to link yet: no global server and no host in another project"
		return
	}
	label := func(slug string) string {
		if name := names[slug]; name != "" {
			return name
		}
		return slug
	}
	var rows []pickRow
	group := "\x00" // no group yet
	for _, t := range targets {
		if t.Project != group {
			group = t.Project
			header := "GLOBAL SERVERS"
			if group != "" {
				header = "PROJECT " + label(group)
			}
			rows = append(rows, pickRow{header: header})
		}
		users := make([]string, len(t.UsedBy))
		for i, slug := range t.UsedBy {
			users[i] = label(slug)
		}
		rows = append(rows, pickRow{target: t, usedBy: strings.Join(users, ", ")})
	}
	m.picker = rows
	m.pickCursor = 1 // the first row is always a header
	m.pickOffset = 0
	m.statusMsg = ""
}

func (m *Model) closePicker() {
	m.picker = nil
	m.confirmPromote = false
}

func (m Model) updatePicker(msg tea.KeyMsg) (Model, tea.Cmd) {
	if m.confirmPromote {
		m.confirmPromote = false
		if msg.String() != "y" && msg.String() != "enter" {
			return m, nil
		}
		target := m.picker[m.pickCursor].target
		m.closePicker()
		return m, func() tea.Msg { return MsgLinkTargetChosen{Target: target} }
	}

	switch msg.String() {
	case "j", "down":
		m.movePick(1)
	case "k", "up":
		m.movePick(-1)
	case "g":
		m.pickCursor = 0
		m.movePick(1)
	case "G":
		m.pickCursor = len(m.picker)
		m.movePick(-1)
	case "enter":
		target := m.picker[m.pickCursor].target
		if target.Project != "" {
			m.confirmPromote = true
			return m, nil
		}
		m.closePicker()
		return m, func() tea.Msg { return MsgLinkTargetChosen{Target: target} }
	case "esc", "q":
		m.closePicker()
	}
	return m, nil
}

// movePick moves the cursor by step, skipping headers and stopping at the ends.
func (m *Model) movePick(step int) {
	for i := m.pickCursor + step; i >= 0 && i < len(m.picker); i += step {
		if m.picker[i].header == "" {
			m.pickCursor = i
			break
		}
	}
	h := m.listHeight()
	if m.pickCursor < m.pickOffset {
		m.pickOffset = m.pickCursor
		// Keep the group header above the first row of the view in sight.
		if m.pickOffset > 0 && m.picker[m.pickOffset-1].header != "" {
			m.pickOffset--
		}
	}
	if m.pickCursor >= m.pickOffset+h {
		m.pickOffset = m.pickCursor - h + 1
	}
}

func (m Model) viewPicker() []string {
	lines := make([]string, 0, m.listHeight())
	for i := m.pickOffset; i < len(m.picker) && len(lines) < m.listHeight(); i++ {
		row := m.picker[i]
		if row.header != "" {
			lines = append(lines, padRight("  "+styles.Key.Render(row.header), m.Width))
			continue
		}
		h := row.target.Host
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
		note := ""
		if row.usedBy != "" {
			note = "linked by " + row.usedBy
		}
		line := "    " +
			styles.Dir.Render(fmt.Sprintf("%-16s", h.Name)) + " " +
			styles.File.Render(fmt.Sprintf("%-34s", endpoint)) + " " +
			styles.Muted.Render(fmt.Sprintf("%-6s", protocol)) + " " +
			styles.Muted.Render(note)
		if lipgloss.Width(line) > m.Width {
			line = lipgloss.NewStyle().MaxWidth(m.Width).Render(line)
		}
		if i == m.pickCursor {
			line = styles.CursorRow.Width(m.Width).Render(padRight(line, m.Width))
		} else {
			line = padRight(line, m.Width)
		}
		lines = append(lines, line)
	}
	return lines
}

func (m Model) pickerStatus() string {
	if m.confirmPromote {
		t := m.picker[m.pickCursor].target
		msg := fmt.Sprintf("  %q becomes a global server; its project keeps using it through a link. ", t.Host.Name)
		return styles.Warn.Render(msg) +
			styles.Dir.Render("[y]") + styles.Muted.Render("es  ") +
			styles.Dir.Render("[n]") + styles.Muted.Render("o")
	}
	return padRight(styles.KeyHints("  [Enter]link  [j/k]move  [Esc]back", styles.Muted), m.Width)
}
