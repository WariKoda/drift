package certtrust

import (
	"strings"

	mousepkg "github.com/WariKoda/drift/internal/tui/mouse"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Update handles keyboard input while the modal owns focus.
func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.SetSize(msg.Width, msg.Height)
	case tea.MouseMsg:
		if delta := mousepkg.WheelDelta(msg); delta != 0 {
			m.Offset += delta
			m.clampOffset()
			return m, nil
		}
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
		case "up", "k":
			m.Offset--
			m.clampOffset()
		case "down", "j":
			m.Offset++
			m.clampOffset()
		case "pgup":
			m.Offset -= m.viewportHeight()
			m.clampOffset()
		case "pgdown":
			m.Offset += m.viewportHeight()
			m.clampOffset()
		case "home":
			m.Offset = 0
		case "end":
			m.Offset = len(m.detailLines())
			m.clampOffset()
		case "enter":
			return m, m.decisionCmd(m.Selection)
		}
	}
	return m, nil
}

func (m Model) buttonAt(x, y int) (Decision, bool) {
	modal := ansi.Strip(m.modal())
	lines := strings.Split(modal, "\n")
	modalWidth := 0
	for _, line := range lines {
		modalWidth = max(modalWidth, lipgloss.Width(line))
	}
	left := max(0, (m.Width-modalWidth)/2)
	top := max(0, (m.Height-len(lines))/2)
	row := y - top
	if row < 0 || row >= len(lines) {
		return Reject, false
	}
	column := x - left
	for decision, label := range []string{"[Reject]", "[Trust for this session]", "[Trust permanently]"} {
		start := strings.Index(lines[row], label)
		if start >= 0 && column >= start && column < start+len(label) {
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
