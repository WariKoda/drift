package hostkeytrust

import (
	"strings"

	mousepkg "github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Update handles keyboard and mouse input while the modal owns focus.
func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.SetSize(msg.Width, msg.Height)
	case tea.MouseMsg:
		if mousepkg.IsLeftPress(msg) {
			if decision, ok := m.buttonAt(msg.X, msg.Y); ok {
				m.Selection = decision
				return m, m.decisionCmd(decision)
			}
		}
	case tea.KeyMsg:
		m.Err = ""
		switch msg.String() {
		case "esc":
			return m, m.decisionCmd(Reject)
		case "tab", "right", "l":
			m.Selection = (m.Selection + 1) % 3
		case "shift+tab", "left", "h":
			m.Selection = (m.Selection + 2) % 3
		case "enter":
			return m, m.decisionCmd(m.Selection)
		}
	}
	return m, nil
}

func (m Model) buttonAt(x, y int) (Decision, bool) {
	lines := strings.Split(ansi.Strip(m.modal()), "\n")
	modalWidth := 0
	for _, line := range lines {
		modalWidth = max(modalWidth, lipgloss.Width(line))
	}
	row := y - max(0, (m.Height-len(lines))/2)
	if row < 0 || row >= len(lines) {
		return Reject, false
	}
	column := x - max(0, (m.Width-modalWidth)/2)
	for decision, label := range buttonLabels {
		index := strings.Index(lines[row], label)
		if index < 0 {
			continue
		}
		// index counts bytes, and the border before it is a multi-byte rune.
		start := lipgloss.Width(lines[row][:index])
		if column >= start && column < start+len(label) {
			return Decision(decision), true
		}
	}
	return Reject, false
}

func (m Model) decisionCmd(decision Decision) tea.Cmd {
	challenge := m.Challenge
	return func() tea.Msg {
		return MsgDecision{Decision: decision, Challenge: challenge}
	}
}
