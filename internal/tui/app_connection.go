package tui

import (
	"context"
	"fmt"

	"github.com/WariKoda/drift/internal/remote"
	tea "github.com/charmbracelet/bubbletea"
)

// A watch belongs to one connection and project, not whichever screen happens
// to receive its result. The root handles it even while a modal owns input.
type msgConnectionEnded struct {
	id      uint64
	project string
	root    string
	conn    remote.Client
	err     error
}

func (a *App) watchConnection(conn remote.Client) tea.Cmd {
	if a.connectionCancel != nil {
		a.connectionCancel()
	}
	a.connectionSeq++
	if conn == nil {
		a.connectionCancel = nil
		return nil
	}
	ctx, cancel := context.WithCancel(context.Background())
	a.connectionCancel = cancel
	msg := msgConnectionEnded{
		id: a.connectionSeq, root: a.state.WorkingDir, conn: conn,
	}
	if a.state.Config != nil {
		msg.project = a.state.Config.ProjectSlug
	}
	return func() tea.Msg {
		select {
		case <-ctx.Done():
			return nil
		case <-conn.Done():
			msg.err = conn.Err()
			return msg
		}
	}
}

func (a *App) connectionEnded(msg msgConnectionEnded) {
	if msg.id != a.connectionSeq || msg.root != a.state.WorkingDir {
		return
	}
	if a.state.Config != nil && msg.project != a.state.Config.ProjectSlug {
		return
	}
	if a.connectionCancel != nil {
		a.connectionCancel()
		a.connectionCancel = nil
	}
	if msg.err == nil {
		return
	}
	browserLost := a.browser.ConnectionLost(msg.conn, msg.err)
	diffLost := a.diffView.ConnectionLost(msg.conn, msg.err)
	if browserLost || diffLost {
		a.globalError = fmt.Sprintf("Remote disconnected: %v. Reconnect and compare before syncing again.", msg.err)
	}
}

// Close releases connections and background observers after Bubble Tea exits.
// It runs outside Update, so waiting for transport shutdown cannot block input.
func (a *App) Close() {
	a.cancelNetworkActivity()
	a.watchConnection(nil)
	if cmd := a.browser.CloseRemote(); cmd != nil {
		cmd()
	}
	if cmd := a.diffView.Close(); cmd != nil {
		cmd()
	}
}
