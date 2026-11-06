package tui

import (
	"testing"

	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/certtrust"
	"github.com/WariKoda/drift/internal/tui/diffview"
)

func TestAbandonedRemoteResultsAreClosedBehindEditorAndModal(t *testing.T) {
	a, _ := editorApp(t, false)
	startEditorForTest(t, a)
	active := a.remoteEditor.Connection()
	prompt := certtrust.New(tlstrust.Challenge{}, 120, 24)
	a.certPrompt = &prompt
	stale, host, _ := loopbackConnection(t)
	cmd := applyEditorMessage(t, a, browser.MsgRemoteLoaded{ID: 999, Host: host, Conn: stale})
	if cmd == nil {
		t.Fatal("abandoned browser result was ignored without cleanup")
	}
	cmd()
	select {
	case <-stale.Done():
	default:
		t.Fatal("abandoned browser connection leaked")
	}
	staleDiff, _, _ := loopbackConnection(t)
	cmd = applyEditorMessage(t, a, diffview.MsgDiffLoaded{RequestID: 999, Conn: staleDiff})
	if cmd == nil {
		t.Fatal("abandoned comparison result was ignored without cleanup")
	}
	cmd()
	select {
	case <-staleDiff.Done():
	default:
		t.Fatal("abandoned comparison connection leaked")
	}
	if cmd := applyEditorMessage(t, a, browser.MsgRemoteLoaded{ID: 999, Conn: active}); cmd != nil {
		t.Fatal("duplicate load attempted to close the editor-owned transport")
	}
	if a.state.Screen != ScreenRemoteEditor || a.certPrompt == nil || a.remoteEditor.Connection() != active {
		t.Fatal("stale result replaced the active editor or modal")
	}
	if _, err := active.Stat("/file.txt"); err != nil {
		t.Fatalf("stale result closed active connection: %v", err)
	}
}
