package diffview

import (
	"context"
	"errors"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/log"
	tea "github.com/charmbracelet/bubbletea"
)

type decorationRequest struct {
	result *diff.DiffResult
	cancel context.CancelFunc
	syntax bool
	name   string
}

// MsgDecorated carries optional presentation data for exactly one request.
type MsgDecorated struct {
	request *decorationRequest
	spans   map[int][]diff.TextSpan
	err     error
	columns int
	syntax  *diff.SyntaxHighlights
}

func (m *Model) startDecoration() tea.Cmd {
	if m.closed || m.showErrors {
		return nil
	}
	session := m.activeSession()
	if session == nil || session.Result == nil {
		return nil
	}
	result := session.Result
	name := session.LocalPath
	if name == "" {
		name = session.RemotePath
	}
	if m.decoration != nil && m.decoration.result == result && m.decoration.syntax == m.preferences.Syntax && m.decoration.name == name {
		return nil
	}
	if m.decoration != nil {
		m.decoration.cancel()
	}
	m.spans = nil
	m.syntax = nil
	m.maxColumns = 0
	m.columnsReady = false
	m.decorationError = ""
	ctx, cancel := context.WithCancel(context.Background())
	request := &decorationRequest{result: result, cancel: cancel, syntax: m.preferences.Syntax, name: name}
	m.decoration = request
	added, removed := result.Counts()
	if result.Binary || len(result.Lines) == 0 {
		cancel()
		return nil
	}
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

func (m *Model) acceptDecoration(msg MsgDecorated) {
	if m.closed || msg.request == nil || msg.request != m.decoration {
		return
	}
	session := m.activeSession()
	if session == nil || session.Result != msg.request.result || m.preferences.Syntax != msg.request.syntax {
		return
	}
	name := session.LocalPath
	if name == "" {
		name = session.RemotePath
	}
	if name != msg.request.name {
		return
	}
	if errors.Is(msg.err, context.Canceled) {
		return
	}
	m.spans = msg.spans
	m.syntax = msg.syntax
	m.maxColumns, m.columnsReady = msg.columns, true
	m.clampHorizontal()
	if msg.err != nil {
		log.Error("diff highlighting failed", "err", msg.err)
		m.decorationError = "Highlighting unavailable: " + msg.err.Error()
	}
}
