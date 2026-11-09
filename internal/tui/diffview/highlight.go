package diffview

import (
	"context"
	"errors"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/log"
	tea "github.com/charmbracelet/bubbletea"
)

// maxDecorations bounds the presentation data kept for files that are no
// longer on screen. Only one decoration request runs at a time.
const maxDecorations = 32

type decorationRequest struct {
	result *diff.DiffResult
	cancel context.CancelFunc
	syntax bool
}

// fileDecoration is the optional presentation data of one comparison.
type fileDecoration struct {
	spans   map[int][]diff.TextSpan
	syntax  *diff.SyntaxHighlights
	columns int
	err     error
}

// MsgDecorated carries optional presentation data for exactly one request.
type MsgDecorated struct {
	request *decorationRequest
	spans   map[int][]diff.TextSpan
	err     error
	columns int
	syntax  *diff.SyntaxHighlights
}

func sessionName(session *diff.Session) string {
	if session.LocalPath != "" {
		return session.LocalPath
	}
	return session.RemotePath
}

// decorationTargets lists the files that need presentation data now: the
// active file first, then the other files on screen in continuous mode.
func (m *Model) decorationTargets() []int {
	var targets []int
	if m.activeIdx >= 0 && m.activeIdx < len(m.sessions) {
		targets = append(targets, m.activeIdx)
	}
	if m.continuous() {
		for _, index := range m.visibleSessions() {
			if index != m.activeIdx {
				targets = append(targets, index)
			}
		}
	}
	return targets
}

func (m Model) decorationFor(index int) *fileDecoration {
	if index < 0 || index >= len(m.sessions) || m.sessions[index].Result == nil {
		return nil
	}
	return m.decorated[m.sessions[index].Result]
}

func decoratable(result *diff.DiffResult) bool {
	return result != nil && !result.Binary && len(result.Lines) > 0
}

// startDecoration queues work for the first target without data. A request for
// a file that is no longer needed is cancelled.
func (m *Model) startDecoration() tea.Cmd {
	if m.closed || m.showErrors {
		return nil
	}
	next := -1
	for _, index := range m.decorationTargets() {
		result := m.sessions[index].Result
		if decoratable(result) && !m.longHidden(index) && m.decorated[result] == nil {
			next = index
			break
		}
	}
	if m.decoration != nil {
		if next >= 0 && m.decoration.result == m.sessions[next].Result && m.decoration.syntax == m.preferences.Syntax {
			return nil
		}
		m.decoration.cancel()
		m.decoration = nil
	}
	if next < 0 {
		return nil
	}
	session := &m.sessions[next]
	result := session.Result
	name := sessionName(session)
	ctx, cancel := context.WithCancel(context.Background())
	request := &decorationRequest{result: result, cancel: cancel, syntax: m.preferences.Syntax}
	m.decoration = request
	added, removed := result.Counts()
	lines := result.Lines
	if request.syntax && m.syntaxCache == nil {
		m.syntaxCache = &diff.SyntaxCache{}
	}
	cache := m.syntaxCache
	return func() tea.Msg {
		defer cancel()
		columns, err := diff.MeasureColumns(ctx, lines)
		var spans map[int][]diff.TextSpan
		if err == nil && added > 0 && removed > 0 {
			spans, err = diff.WordHighlights(ctx, lines)
		}
		var syntax *diff.SyntaxHighlights
		if err == nil && request.syntax {
			syntax, err = cache.Highlight(ctx, name, lines)
		}
		return MsgDecorated{request: request, spans: spans, columns: columns, err: err, syntax: syntax}
	}
}

// acceptDecoration stores the result of the current request only. Preference
// changes cancel and replace the request, so its identity is sufficient.
func (m *Model) acceptDecoration(msg MsgDecorated) {
	if m.closed || msg.request == nil || msg.request != m.decoration {
		return
	}
	m.decoration = nil
	if errors.Is(msg.err, context.Canceled) || msg.request.syntax != m.preferences.Syntax {
		return
	}
	if m.decorated == nil {
		m.decorated = make(map[*diff.DiffResult]*fileDecoration)
	}
	m.decorated[msg.request.result] = &fileDecoration{spans: msg.spans, syntax: msg.syntax, columns: msg.columns, err: msg.err}
	if msg.err != nil {
		log.Error("diff highlighting failed", "err", msg.err)
	}
	if len(m.decorated) > maxDecorations {
		keep := make(map[*diff.DiffResult]bool)
		for _, index := range m.decorationTargets() {
			keep[m.sessions[index].Result] = true
		}
		for result := range m.decorated {
			if !keep[result] {
				delete(m.decorated, result)
			}
		}
	}
	m.clampHorizontal()
}

// resetDecorations drops all presentation data, for example after the syntax
// preference changed.
func (m *Model) resetDecorations() {
	if m.decoration != nil {
		m.decoration.cancel()
		m.decoration = nil
	}
	m.decorated = nil
}

// columnLimit is the widest measured code line among the files on screen.
func (m Model) columnLimit() (int, bool) {
	limit, ready := 0, false
	targets := []int{m.activeIdx}
	if m.continuous() {
		targets = append(targets, m.visibleSessions()...)
	}
	for _, index := range targets {
		if decoration := m.decorationFor(index); decoration != nil {
			limit, ready = max(limit, decoration.columns), true
		}
	}
	return limit, ready
}
