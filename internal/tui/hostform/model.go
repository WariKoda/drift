// Package hostform implements the create/edit host form screen.
package hostform

import (
	"fmt"
	"strconv"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tui/textfield"
	tea "github.com/charmbracelet/bubbletea"
)

// Protocol enumerates the supported remote protocols.
type Protocol int

const (
	ProtoSFTP Protocol = iota
	ProtoFTP
	ProtoFTPS
)

func (p Protocol) String() string {
	return [...]string{"sftp", "ftp", "ftps"}[p]
}

// AuthType enumerates the three SSH auth methods.
type AuthType int

const (
	AuthKeyfile AuthType = iota
	AuthPassword
	AuthAgent
)

func (a AuthType) String() string {
	return [...]string{"keyfile", "password", "agent"}[a]
}

// Field indices for m.fields slice and focus tracking.
const (
	fName        = 0
	fHostname    = 1
	fPort        = 2
	fUser        = 3
	fAuthType    = 4 // toggle — no text field
	fKeyFile     = 5
	fPassphrase  = 6
	fPassword    = 7
	fRootPath    = 8
	fScope       = 9  // toggle — no text field
	fProtocol    = 10 // toggle — no text field
	fMappings    = 11 // virtual row — opens mapping sub-screen
	fInsecureTLS = 12 // toggle — no text field (ftps only)
)

// subScreen tracks which panel is currently shown.
type subScreen int

const (
	subMain        subScreen = iota
	subMappingList           // list of mappings for this host
	subMappingEdit           // edit a single mapping (new or existing)
)

// Model is the host create/edit form.
type Model struct {
	fields      [9]*textfield.TextField // indices 0–8 (fName..fRootPath); toggles have no text field
	authType    AuthType
	protocol    Protocol
	insecureTLS bool
	scope       config.HostScope

	focusRow int // which row is active (maps to visibleRows())

	isEdit      bool
	oldName     string
	projectRoot string
	errMsg      string

	Width  int
	Height int

	// per-host mappings
	mappings []config.Mapping

	// sub-screen state
	sub           subScreen
	mapCursor     int
	mapConfirmDel bool
	editIdx       int // -1 = new mapping
	editFields    [2]*textfield.TextField
	editFocusRow  int
}

// New returns a blank form for a new host.
func New(scope config.HostScope, projectRoot string, width, height int) Model {
	m := Model{scope: scope, projectRoot: projectRoot, Width: width, Height: height, editIdx: -1}
	m.initFields()
	m.fields[fName].Focused = true
	return m
}

// NewEdit returns a form pre-filled with an existing host's values.
func NewEdit(h config.Host, scope config.HostScope, projectRoot string, width, height int) Model {
	m := Model{isEdit: true, oldName: h.Name, scope: scope, projectRoot: projectRoot, Width: width, Height: height, editIdx: -1}
	m.initFields()

	m.fields[fName].SetValue(h.Name)
	m.fields[fHostname].SetValue(h.Hostname)
	if h.Port != 0 && h.Port != 22 {
		m.fields[fPort].SetValue(strconv.Itoa(h.Port))
	}
	m.fields[fUser].SetValue(h.User)
	m.fields[fRootPath].SetValue(h.RootPath)

	switch h.Protocol {
	case "ftp":
		m.protocol = ProtoFTP
	case "ftps":
		m.protocol = ProtoFTPS
	}
	m.insecureTLS = h.InsecureTLS

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

	m.mappings = append([]config.Mapping(nil), h.Mappings...)

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
	bw := w - 22
	if bw < 20 {
		bw = 20
	}
	for i, f := range m.fields {
		if f != nil && i != fPort {
			f.Width = bw
		}
	}
	if m.editFields[0] != nil {
		m.editFields[0].Width = bw
		m.editFields[1].Width = bw
	}
}

func (m *Model) initFields() {
	bw := m.Width - 22
	if bw < 20 {
		bw = 20
	}
	m.fields[fName] = &textfield.TextField{Label: "Name", Width: bw, Placeholder: "prod", MaxLen: 64}
	m.fields[fHostname] = &textfield.TextField{Label: "Hostname", Width: bw, Placeholder: "example.com"}
	portPlaceholder := "22"
	if m.protocol == ProtoFTP || m.protocol == ProtoFTPS {
		portPlaceholder = "21"
	}
	m.fields[fPort] = &textfield.TextField{Label: "Port", Width: 6, Placeholder: portPlaceholder, MaxLen: 5}
	m.fields[fUser] = &textfield.TextField{Label: "User", Width: bw, Placeholder: "deploy", MaxLen: 64}
	m.fields[fKeyFile] = &textfield.TextField{Label: "Key File", Width: bw, Placeholder: "~/.ssh/id_ed25519"}
	m.fields[fPassphrase] = &textfield.TextField{Label: "Passphrase", Width: bw, Password: true}
	m.fields[fPassword] = &textfield.TextField{Label: "Password", Width: bw, Password: true, Placeholder: "or $ENV_VAR"}
	m.fields[fRootPath] = &textfield.TextField{Label: "Root Path", Width: bw, Placeholder: "/var/www"}
}

// visibleRows returns the ordered focus positions for the current auth type.
func (m Model) visibleRows() []int {
	rows := []int{fName, fHostname, fPort, fUser, fProtocol}
	switch m.protocol {
	case ProtoSFTP:
		rows = append(rows, fAuthType)
		switch m.authType {
		case AuthKeyfile:
			rows = append(rows, fKeyFile, fPassphrase)
		case AuthPassword:
			rows = append(rows, fPassword)
		}
	case ProtoFTP, ProtoFTPS:
		rows = append(rows, fPassword)
		if m.protocol == ProtoFTPS {
			rows = append(rows, fInsecureTLS)
		}
	}
	return append(rows, fRootPath, fMappings, fScope)
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

func (m *Model) applyEditFocus() {
	for i, f := range m.editFields {
		if f != nil {
			f.Focused = (i == m.editFocusRow)
		}
	}
}

// openMappingEdit prepares the edit sub-form for index idx (-1 = new).
func (m *Model) openMappingEdit(idx int) {
	bw := m.Width - 22
	if bw < 20 {
		bw = 20
	}
	m.editIdx = idx
	m.editFocusRow = 0
	m.editFields[0] = &textfield.TextField{Label: "Local Path", Width: bw, Placeholder: "plugins/plugin1", Focused: true}
	m.editFields[1] = &textfield.TextField{Label: "Deploy Path", Width: bw, Placeholder: "custom/plugins/plugin1"}
	if idx >= 0 && idx < len(m.mappings) {
		m.editFields[0].SetValue(m.mappings[idx].Local)
		m.editFields[1].SetValue(m.mappings[idx].Remote)
	}
	m.sub = subMappingEdit
}

// saveMappingEdit writes the current edit fields back to m.mappings.
// Returns false if required fields are empty.
func (m *Model) saveMappingEdit() bool {
	local := m.editFields[0].Value()
	remote := m.editFields[1].Value()
	if local == "" || remote == "" {
		return false
	}
	mp := config.Mapping{Local: local, Remote: remote}
	if m.editIdx < 0 {
		m.mappings = append(m.mappings, mp)
		m.mapCursor = len(m.mappings) - 1
	} else {
		m.mappings[m.editIdx] = mp
	}
	return true
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

	defaultPort := 22
	if m.protocol == ProtoFTP || m.protocol == ProtoFTPS {
		defaultPort = 21
	}
	port := defaultPort
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
		Protocol: m.protocol.String(),
		Mappings: append([]config.Mapping(nil), m.mappings...),
	}
	if m.protocol == ProtoFTP || m.protocol == ProtoFTPS {
		h.Auth = config.Auth{Type: "password", Password: m.fields[fPassword].Value()}
		if m.protocol == ProtoFTPS {
			h.InsecureTLS = m.insecureTLS
		}
		return h, nil
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
