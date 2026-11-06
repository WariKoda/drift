package tui

import (
	"testing"

	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/certtrust"
	"github.com/WariKoda/drift/internal/tui/remoteeditor"
	tea "github.com/charmbracelet/bubbletea"
)

func TestEditorFTPSCertificateRetryOnlyReconnectsAndChecks(t *testing.T) {
	a, server := editorApp(t, true)
	_, _, cmd := startEditorForTest(t, a)
	cmd = applyEditorMessage(t, a, runEditorCommand(t, cmd)) // process end → capture
	applyEditorMessage(t, a, runEditorCommand(t, cmd))
	host := a.remoteEditor.Host()
	endpoint, err := tlstrust.NormalizeEndpoint(host.Protocol, host.Hostname, host.Port)
	if err != nil {
		t.Fatal(err)
	}
	if err := a.trust.Reset(endpoint); err != nil {
		t.Fatal(err)
	}
	cmd = applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("c")})
	failed := runEditorCommand(t, cmd).(remoteeditor.MsgResult)
	if failed.Result.Err == nil {
		t.Fatal("expected new FTPS trust challenge")
	}
	applyEditorMessage(t, a, failed)
	if a.certPrompt == nil || a.pendingTrust.kind != trustOperationRemoteEdit {
		t.Fatal("editor reconnect did not open its certificate prompt")
	}
	cmd = applyEditorMessage(t, a, certtrust.MsgDecision{Decision: certtrust.TrustSession, Challenge: a.certPrompt.Challenge})
	connected := runEditorCommand(t, cmd).(remoteeditor.MsgResult)
	if connected.Result.Err != nil || connected.Conn == nil {
		t.Fatalf("trusted reconnect: %+v", connected)
	}
	cmd = applyEditorMessage(t, a, connected)
	if duplicate := applyEditorMessage(t, a, connected); duplicate != nil {
		duplicate()
	}
	checked := runEditorCommand(t, cmd).(remoteeditor.MsgResult)
	if checked.Result.Err != nil {
		t.Fatal(checked.Result.Err)
	}
	if next := applyEditorMessage(t, a, checked); next != nil || server.CommandCount("STOR") != 0 {
		t.Fatal("trust/reconnect/check retried a transfer")
	}
	cmd = applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("u")})
	saved := runEditorCommand(t, cmd).(remoteeditor.MsgResult)
	if saved.Result.Err != nil || !saved.Result.Saved {
		t.Fatalf("confirmed upload: %+v", saved.Result)
	}
	cmd = applyEditorMessage(t, a, saved)
	applyEditorMessage(t, a, runEditorCommand(t, cmd))
	if a.state.Screen != ScreenBrowser {
		t.Fatal("upload did not return to browser")
	}
}

func TestReconnectAfterEditorFailureCannotRestoreStaleUploadConsent(t *testing.T) {
	a, server := editorApp(t, false)
	_, _, cmd := startEditorForTest(t, a)
	cmd = applyEditorMessage(t, a, runEditorCommand(t, cmd))
	applyEditorMessage(t, a, runEditorCommand(t, cmd))
	t.Setenv("DRIFT_GUI_EDITOR_FAIL", "1")
	cmd = applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
	applyEditorMessage(t, a, runEditorCommand(t, cmd))
	cmd = applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("c")})
	cmd = applyEditorMessage(t, a, runEditorCommand(t, cmd))
	applyEditorMessage(t, a, runEditorCommand(t, cmd))
	if cmd := applyEditorMessage(t, a, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("u")}); cmd != nil {
		t.Fatal("reconnect made the failed editor's stale snapshot uploadable")
	}
	if server.CommandCount("STOR") != 0 {
		t.Fatal("failed editor caused an upload")
	}
}
