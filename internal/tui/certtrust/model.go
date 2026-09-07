// Package certtrust implements the modal used to approve an FTPS certificate
// exception.
package certtrust

import (
	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
)

// Decision is the scope selected by the user.
type Decision int

const (
	Reject Decision = iota
	TrustSession
	TrustPermanently
)

// MsgDecision is emitted when the user confirms or rejects a challenge.
type MsgDecision struct {
	Decision  Decision
	Challenge tlstrust.Challenge
}

// Model holds one certificate challenge.
type Model struct {
	Challenge tlstrust.Challenge
	Selection Decision
	Width     int
	Height    int
	Offset    int
	Err       string
}

// New creates a prompt with Reject selected.
func New(challenge tlstrust.Challenge, width, height int) Model {
	return Model{Challenge: challenge, Selection: Reject, Width: width, Height: height}
}

// Init implements tea.Model.
func (m Model) Init() tea.Cmd { return nil }

// SetSize updates the available terminal dimensions.
func (m *Model) SetSize(width, height int) {
	m.Width = width
	m.Height = height
	m.clampOffset()
}

// SetError keeps the prompt open and displays a persistence error.
func (m *Model) SetError(err error) {
	if err == nil {
		m.Err = ""
		return
	}
	m.Err = err.Error()
}

func (m *Model) clampOffset() {
	maximum := max(0, len(m.detailLines())-m.viewportHeight())
	m.Offset = min(max(0, m.Offset), maximum)
}

func (m Model) viewportHeight() int {
	return max(3, m.Height-12)
}
