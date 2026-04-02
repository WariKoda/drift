// Package hostform implements the create/edit host form screen.
package hostform

import (
	"fmt"
	"strconv"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/yourusername/drift/internal/config"
)

// AuthType enumerates the three SSH auth methods.
type AuthType int

const (
	AuthKeyfile  AuthType = iota
	AuthPassword
	AuthAgent
)

func (a AuthType) String() string {
	return [...]string{"keyfile", "password", "agent"}[a]
}

// Field indices for m.fields slice and focus tracking.
const (
	fName       = 0
	fHostname   = 1
	fPort       = 2
	fUser       = 3
	fAuthType   = 4 // toggle — no text field
	fKeyFile    = 5
	fPassphrase = 6
	fPassword   = 7
	fRootPath   = 8
	fScope      = 9 // toggle — no text field
)

// Model is the host create/edit form.
type Model struct {
	fields   [9]*TextField // indices 0–8 (fName..fRootPath); fAuthType/fScope are toggles
	authType AuthType
	scope    config.HostScope

	focusRow int // which row is active (maps to visibleRows())

	isEdit      bool
	oldName     string
	projectRoot string // for scope toggle label
	errMsg      string

	Width  int
	Height int
}

// New returns a blank form for a new host.
func New(scope config.HostScope, projectRoot string, width, height int) Model {
	m := Model{scope: scope, projectRoot: projectRoot, Width: width, Height: height}
	m.initFields()
	m.fields[fName].Focused = true
	return m
}

// NewEdit returns a form pre-filled with an existing host's values.
func NewEdit(h config.Host, scope config.HostScope, projectRoot string, width, height int) Model {
	m := Model{isEdit: true, oldName: h.Name, scope: scope, projectRoot: projectRoot, Width: width, Height: height}
	m.initFields()

	m.fields[fName].SetValue(h.Name)
	m.fields[fHostname].SetValue(h.Hostname)
	if h.Port != 0 && h.Port != 22 {
		m.fields[fPort].SetValue(strconv.Itoa(h.Port))
	}
	m.fields[fUser].SetValue(h.User)
	m.fields[fRootPath].SetValue(h.RootPath)

	switch h.Auth.Type {
	case "password":
		m.authType = AuthPassword
		m.fields[fPassword].SetValue(h.Auth.Password)
	case "agent":
		m.authType = AuthAgent
	default:
		m.authType = AuthKeyfile
		m.fields[fKeyFile].SetValue(h.Auth.KeyFile)
		m.fields[fPassphrase].SetValue(h.Auth.Passphrase)
	}

	m.fields[fName].Focused = true
	return m
}

// Init satisfies the bubbletea sub-model convention.
func (m Model) Init() tea.Cmd { return nil }

// SetErr sets a validation/save error message on the form.
func (m *Model) SetErr(msg string) { m.errMsg = msg }

// SetSize updates terminal dimensions.
func (m *Model) SetSize(w, h int) {
	m.Width = w
	m.Height = h
	// Recompute field widths
	bw := w - 22
	if bw < 20 {
		bw = 20
	}
	for i, f := range m.fields {
		if f != nil && i != fPort {
			f.Width = bw
		}
	}
}

func (m *Model) initFields() {
	bw := m.Width - 22
	if bw < 20 {
		bw = 20
	}
	m.fields[fName] = &TextField{Label: "Name", Width: bw, Placeholder: "prod", MaxLen: 64}
	m.fields[fHostname] = &TextField{Label: "Hostname", Width: bw, Placeholder: "example.com"}
	m.fields[fPort] = &TextField{Label: "Port", Width: 6, Placeholder: "22", MaxLen: 5}
	m.fields[fUser] = &TextField{Label: "User", Width: bw, Placeholder: "deploy", MaxLen: 64}
	m.fields[fKeyFile] = &TextField{Label: "Key File", Width: bw, Placeholder: "~/.ssh/id_ed25519"}
	m.fields[fPassphrase] = &TextField{Label: "Passphrase", Width: bw, Password: true}
	m.fields[fPassword] = &TextField{Label: "Password", Width: bw, Password: true, Placeholder: "or $ENV_VAR"}
	m.fields[fRootPath] = &TextField{Label: "Root Path", Width: bw, Placeholder: "/var/www"}
}

// visibleRows returns the ordered focus positions for the current auth type.
// fAuthType and fScope are "virtual" rows (toggles, no text field).
func (m Model) visibleRows() []int {
	rows := []int{fName, fHostname, fPort, fUser, fAuthType}
	switch m.authType {
	case AuthKeyfile:
		rows = append(rows, fKeyFile, fPassphrase)
	case AuthPassword:
		rows = append(rows, fPassword)
	}
	return append(rows, fRootPath, fScope)
}

func (m *Model) applyFocus() {
	rows := m.visibleRows()
	cur := -1
	if m.focusRow >= 0 && m.focusRow < len(rows) {
		cur = rows[m.focusRow]
	}
	for i, f := range m.fields {
		if f != nil {
			f.Focused = (i == cur)
		}
	}
}

// toHost validates and builds a config.Host from the form values.
func (m Model) toHost() (config.Host, error) {
	name := m.fields[fName].Value()
	if name == "" {
		return config.Host{}, fmt.Errorf("Name is required")
	}
	hostname := m.fields[fHostname].Value()
	if hostname == "" {
		return config.Host{}, fmt.Errorf("Hostname is required")
	}
	root := m.fields[fRootPath].Value()
	if root == "" {
		return config.Host{}, fmt.Errorf("Root Path is required")
	}

	port := 22
	if p := m.fields[fPort].Value(); p != "" {
		n, err := strconv.Atoi(p)
		if err != nil || n < 1 || n > 65535 {
			return config.Host{}, fmt.Errorf("Port must be 1–65535")
		}
		port = n
	}

	h := config.Host{
		Name:     name,
		Hostname: hostname,
		Port:     port,
		User:     m.fields[fUser].Value(),
		RootPath: root,
	}
	switch m.authType {
	case AuthKeyfile:
		h.Auth = config.Auth{
			Type:       "keyfile",
			KeyFile:    m.fields[fKeyFile].Value(),
			Passphrase: m.fields[fPassphrase].Value(),
		}
	case AuthPassword:
		h.Auth = config.Auth{
			Type:     "password",
			Password: m.fields[fPassword].Value(),
		}
	case AuthAgent:
		h.Auth = config.Auth{Type: "agent"}
	}
	return h, nil
}
