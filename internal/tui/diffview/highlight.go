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
}

// MsgDecorated carries optional presentation data for exactly one request.
type MsgDecorated struct {
	request *decorationRequest
	spans   map[int][]diff.TextSpan
	err     error
	columns int
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
	if m.decoration != nil && m.decoration.result == result {
		return nil
	}
	if m.decoration != nil {
		m.decoration.cancel()
	}
	m.spans = nil
	m.maxColumns = 0
	m.columnsReady = false
	m.decorationError = ""
	ctx, cancel := context.WithCancel(context.Background())
	request := &decorationRequest{result: result, cancel: cancel}
	m.decoration = request
	added, removed := result.Counts()
	if result.Binary || len(result.Lines) == 0 {
		cancel()
		return nil
	}
	lines := result.Lines
	return func() tea.Msg {
		defer cancel()
		columns, err := diff.MeasureColumns(ctx, lines)
		var spans map[int][]diff.TextSpan
		if err == nil && added > 0 && removed > 0 {
			spans, err = diff.WordHighlights(ctx, lines)
		}
		return MsgDecorated{request: request, spans: spans, columns: columns, err: err}
	}
}

func (m *Model) acceptDecoration(msg MsgDecorated) {
	if msg.request == nil || msg.request != m.decoration {
		return
	}
	if errors.Is(msg.err, context.Canceled) {
		return
	}
	if msg.err != nil {
		log.Error("diff highlighting failed", "err", msg.err)
		m.decorationError = "Highlighting unavailable: " + msg.err.Error()
		return
	}
	m.spans = msg.spans
	m.maxColumns, m.columnsReady = msg.columns, true
	m.clampHorizontal()
}
