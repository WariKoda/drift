// Package hostkeytrust implements the modal used to approve the key of an SSH
// host that known_hosts does not list.
package hostkeytrust

import (
	"github.com/WariKoda/drift/internal/ssh"
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
	Challenge ssh.HostKeyChallenge
}

// Model holds one host key challenge.
type Model struct {
	Challenge ssh.HostKeyChallenge
	Selection Decision
	Width     int
	Height    int
	Err       string
}

// New creates a prompt with Reject selected.
func New(challenge ssh.HostKeyChallenge, width, height int) Model {
	return Model{Challenge: challenge, Selection: Reject, Width: width, Height: height}
}

// Init satisfies the sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// SetSize updates the available terminal dimensions.
func (m *Model) SetSize(width, height int) {
	m.Width = width
	m.Height = height
}

// SetError keeps the prompt open and displays a persistence error.
func (m *Model) SetError(err error) {
	if err == nil {
		m.Err = ""
		return
	}
	m.Err = err.Error()
}
