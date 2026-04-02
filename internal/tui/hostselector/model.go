// Package hostselector implements the host picker modal shown when the user presses [s].
package hostselector

import (
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/yourusername/drift/internal/config"
	"github.com/yourusername/drift/internal/styles"
)

// MsgHostChosen is emitted when the user selects a host.
type MsgHostChosen struct{ Host config.Host }

// MsgSelectorCancelled is emitted when the user presses Esc.
type MsgSelectorCancelled struct{}

// Model is the host selector modal.
type Model struct {
	hosts    []config.Host // all available hosts (merged)
	filtered []config.Host // hosts matching current query
	query    string
	cursor   int
	Width    int
	Height   int
}

// New creates a selector from the merged config hosts.
func New(cfg *config.MergedConfig, width, height int) Model {
	var hosts []config.Host
	for _, h := range cfg.Hosts {
		hosts = append(hosts, h)
	}
	m := Model{hosts: hosts, Width: width, Height: height}
	m.applyFilter()
	return m
}

// Init satisfies the sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case "esc", "ctrl+c":
		return m, func() tea.Msg { return MsgSelectorCancelled{} }

	case "enter":
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			h := m.filtered[m.cursor]
			return m, func() tea.Msg { return MsgHostChosen{Host: h} }
		}

	case "j", "down":
		if m.cursor < len(m.filtered)-1 {
			m.cursor++
		}

	case "k", "up":
		if m.cursor > 0 {
			m.cursor--
		}

	case "backspace", "ctrl+h":
		if len(m.query) > 0 {
			r := []rune(m.query)
			m.query = string(r[:len(r)-1])
			m.applyFilter()
		}

	default:
		for _, r := range msg.Runes {
			m.query += string(r)
		}
		m.applyFilter()
	}
	return m, nil
}

func (m *Model) applyFilter() {
	q := strings.ToLower(m.query)
	m.filtered = nil
	for _, h := range m.hosts {
		if q == "" || strings.Contains(strings.ToLower(h.Name), q) ||
			strings.Contains(strings.ToLower(h.Hostname), q) {
			m.filtered = append(m.filtered, h)
		}
	}
	if m.cursor >= len(m.filtered) {
		m.cursor = max(0, len(m.filtered)-1)
	}
}

func (m Model) View() string {
	// Modal centered in the terminal
	boxWidth := 54
	if m.Width < boxWidth+4 {
		boxWidth = m.Width - 4
	}

	var sb strings.Builder

	border := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(styles.ColorDir).
		Padding(0, 1).
		Width(boxWidth)

	var inner strings.Builder

	// Title
	inner.WriteString(styles.Header.Render("Select host"))
	inner.WriteByte('\n')

	// Search input
	query := m.query + "█"
	if m.query == "" {
		query = styles.Muted.Render("type to filter…")
	}
	inner.WriteString(styles.Sep.Render("  > ") + query)
	inner.WriteByte('\n')
	inner.WriteString(styles.Sep.Render(strings.Repeat("─", boxWidth-2)))
	inner.WriteByte('\n')

	// Host list
	if len(m.filtered) == 0 {
		inner.WriteString(styles.Muted.Render("  no hosts found"))
		inner.WriteByte('\n')
	} else {
		for i, h := range m.filtered {
			port := ""
			if h.Port != 0 && h.Port != 22 {
				port = lipgloss.NewStyle().Foreground(styles.ColorMuted).Render(
					":" + itoa(h.Port),
				)
			}
			name := styles.Dir.Render(padStr(h.Name, 16))
			host := styles.File.Render(h.Hostname) + port

			line := "  " + name + " " + host
			if i == m.cursor {
				line = styles.CursorRow.Width(boxWidth).Render(padRight(line, boxWidth))
			}
			inner.WriteString(line)
			inner.WriteByte('\n')
		}
	}

	inner.WriteString("\n" + styles.Muted.Render("  [Enter]select  [Esc]cancel"))

	sb.WriteString(border.Render(inner.String()))
	return sb.String()
}

func itoa(n int) string {
	if n == 0 {
		return ""
	}
	s := ""
	for n > 0 {
		s = string(rune('0'+n%10)) + s
		n /= 10
	}
	return s
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

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
