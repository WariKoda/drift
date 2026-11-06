package tui

import (
	"fmt"

	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remoteedit"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/remoteeditor"
	tea "github.com/charmbracelet/bubbletea"
)

type remoteEditorRequest struct {
	request browser.MsgEditRequested
	root    string
	project string
}

func (a App) startRemoteEditor(msg browser.MsgEditRequested) (tea.Model, tea.Cmd) {
	if a.state.Screen != ScreenBrowser || a.loader.Active() || !a.browser.TakeEditRequest(msg) {
		if msg.Conn != nil && (msg.Conn == a.remoteEditor.Connection() || msg.Conn == a.browser.Connection() || msg.Conn == a.diffView.Connection()) {
			return a, nil
		}
		return a, func() tea.Msg {
			if msg.Conn != nil {
				if err := msg.Conn.Close(); err != nil {
					log.Error("close stale edit request", "err", err)
				}
			}
			return nil
		}
	}
	cfg := a.state.Config
	if cfg == nil || cfg.Editor == nil {
		return a, a.browser.ResumeEdit(msg, msg.Conn, nil, false, "Configure [editor] in the global config first")
	}
	a.abandonDiffRequest()
	a.editRequest = &remoteEditorRequest{request: msg, root: a.state.WorkingDir, project: cfg.ProjectSlug}
	request := remoteedit.Request{Host: msg.Host, RemotePath: msg.Path, ProjectRoot: cfg.ProjectRoot, Mappings: cfg.Mappings}
	if request.ProjectRoot == "" {
		request.ProjectRoot = a.state.WorkingDir
	}
	a.remoteEditor = remoteeditor.New(request, cfg.Editor, msg.Conn, a.trust, a.state.TermWidth, a.state.TermHeight)
	a.state.Screen = ScreenRemoteEditor
	cmd := a.remoteEditor.Init()
	label, tracker, _ := a.remoteEditor.LoadingActivity()
	return a, tea.Batch(cmd, a.startNetworkActivity(activityRemoteEdit, label, tracker))
}

func (a App) updateRemoteEditor(msg tea.Msg) (tea.Model, tea.Cmd) {
	if result, ok := msg.(remoteeditor.MsgResult); ok {
		if a.state.Screen != ScreenRemoteEditor || a.editRequest == nil || a.editRequest.root != a.state.WorkingDir ||
			a.state.Config == nil || a.editRequest.project != a.state.Config.ProjectSlug {
			return a, result.Discard(nil, a.browser.Connection(), a.diffView.Connection(), a.remoteEditor.Connection())
		}
		if !a.remoteEditor.Accepts(result) {
			_, cmd := a.remoteEditor.Update(result)
			return a, cmd
		}
	} else if a.state.Screen != ScreenRemoteEditor {
		return a, nil
	}
	before := a.remoteEditor.Connection()
	var cmd tea.Cmd
	a.remoteEditor, cmd = a.remoteEditor.Update(msg)
	if a.activity == activityRemoteEdit {
		a.finishNetworkActivity(activityRemoteEdit)
	}
	if label, tracker, active := a.remoteEditor.LoadingActivity(); active {
		cmd = tea.Batch(cmd, a.startNetworkActivity(activityRemoteEdit, label, tracker))
	}
	if conn := a.remoteEditor.Connection(); conn != before {
		cmd = tea.Batch(cmd, a.watchConnection(conn))
	}
	if result, ok := msg.(remoteeditor.MsgResult); ok && result.Result.Err != nil {
		a.openCertificatePrompt(trustOperationRemoteEdit, a.remoteEditor.Host(), result.Result.Err)
	}
	return a, cmd
}

func (a App) finishRemoteEditor(msg remoteeditor.MsgClosed) (tea.Model, tea.Cmd) {
	pending := a.editRequest
	if a.state.Screen != ScreenRemoteEditor || !a.remoteEditor.AcceptsClosed(msg) || pending == nil ||
		pending.root != a.state.WorkingDir || a.state.Config == nil || pending.project != a.state.Config.ProjectSlug {
		return a, msg.Discard(a.browser.Connection(), a.diffView.Connection(), a.remoteEditor.Connection())
	}
	a.finishNetworkActivity(activityRemoteEdit)
	status := msg.Status
	if msg.Err != nil {
		log.Error("finish remote editor", "host", pending.request.Host.Name, "err", msg.Err)
		status += fmt.Sprintf("; cleanup failed (working copy: %s): %v", msg.File, msg.Err)
	}
	cmd := a.browser.ResumeEdit(pending.request, msg.Conn, msg.Info, msg.Saved, status)
	a.editRequest = nil
	a.remoteEditor = remoteeditor.Model{}
	a.state.Screen = ScreenBrowser
	return a, tea.Batch(cmd, a.watchConnection(msg.Conn))
}
