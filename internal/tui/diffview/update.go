package diffview

import (
	tea "github.com/charmbracelet/bubbletea"
)

// MsgBackToBrowser is sent when the user quits the diff view.
type MsgBackToBrowser struct{}

func (m Model) Update(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.Width = msg.Width
		m.Height = msg.Height
		m.clampScroll()

	case tea.KeyMsg:
		return m.handleKey(msg)

	case MsgSynced:
		m.reloadSession(msg.SessionIdx)
		m.scroll = 0

	case MsgSyncError:
		// error will be visible in status bar via activeSession().Err
		if s := m.activeSession(); s != nil {
			s.Err = msg.Err
		}
	}
	return m, nil
}

func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {

	// ── Navigation ────────────────────────────────────
	case "j", "down":
		m.scroll++
		m.clampScroll()

	case "k", "up":
		m.scroll--
		m.clampScroll()

	case "ctrl+d":
		m.scroll += m.viewportHeight() / 2
		m.clampScroll()

	case "ctrl+u":
		m.scroll -= m.viewportHeight() / 2
		m.clampScroll()

	case "g":
		m.scroll = 0

	case "G":
		m.scroll = m.totalLines()
		m.clampScroll()

	// ── File navigation ───────────────────────────────
	case "tab", "n":
		if m.activeIdx < len(m.sessions)-1 {
			m.activeIdx++
			m.scroll = 0
		}

	case "shift+tab", "p":
		if m.activeIdx > 0 {
			m.activeIdx--
			m.scroll = 0
		}

	// ── Jump to next diff hunk ─────────────────────────
	case "]":
		m.jumpNextHunk()
	case "[":
		m.jumpPrevHunk()

	// ── Sync operations ───────────────────────────────
	case "u":
		if s := m.activeSession(); s != nil && s.Result != nil && !s.Result.RemoteOnly {
			return m, m.uploadCmd(m.activeIdx)
		}

	case "d":
		if s := m.activeSession(); s != nil && s.Result != nil && !s.Result.LocalOnly {
			return m, m.downloadCmd(m.activeIdx)
		}

	// ── Quit ──────────────────────────────────────────
	case "q", "esc":
		return m, func() tea.Msg { return MsgBackToBrowser{} }
	}

	return m, nil
}

// jumpNextHunk moves scroll to the next diff hunk start.
func (m *Model) jumpNextHunk() {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return
	}
	lines := s.Result.Lines
	for i := m.scroll + 1; i < len(lines); i++ {
		if lines[i].Kind != 0 { // non-equal
			m.scroll = i
			return
		}
	}
}

// jumpPrevHunk moves scroll to the previous diff hunk start.
func (m *Model) jumpPrevHunk() {
	s := m.activeSession()
	if s == nil || s.Result == nil {
		return
	}
	lines := s.Result.Lines
	for i := m.scroll - 1; i >= 0; i-- {
		if lines[i].Kind != 0 {
			m.scroll = i
			return
		}
	}
}
