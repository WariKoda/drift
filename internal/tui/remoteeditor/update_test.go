package remoteeditor

import (
	"errors"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/remoteedit"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func key(s string) tea.KeyMsg { return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)} }

func TestPreparationStartsEditorAndWaitsForProcessEnd(t *testing.T) {
	m := New(remoteedit.Request{}, &config.Editor{Command: []string{"nano", "{file}"}}, nil, nil, 80, 24)
	_ = m.Init()
	result := MsgResult{Session: m.session, ID: m.seq, kind: prepare, Result: remoteedit.Result{File: "/tmp/work/file"}}
	m, cmd := m.Update(result)
	if cmd == nil || !m.editing || m.busy || m.kind != process || m.seq != result.ID+1 {
		t.Fatalf("preparation did not enter process phase: %+v", m)
	}
	seq := m.seq
	for _, s := range []string{"u", "q", "c", "P", "H", "e", "k", "d"} {
		var next tea.Cmd
		m, next = m.Update(key(s))
		if next != nil || !m.editing || m.seq != seq {
			t.Fatalf("%q interrupted or transferred during editing", s)
		}
	}
	m, cmd = m.Update(MsgResult{Session: m.session, ID: m.seq, kind: process})
	if cmd == nil || !m.busy || m.editing || m.kind != capture {
		t.Fatalf("process end did not start capture: %+v", m)
	}
}

func TestChangesRequireExplicitUploadKey(t *testing.T) {
	m := New(remoteedit.Request{}, &config.Editor{Command: []string{"nano", "{file}"}}, nil, nil, 80, 24)
	m.kind, m.seq, m.busy, m.ready = capture, 5, true, true
	m, cmd := m.Update(MsgResult{Session: m.session, ID: 5, kind: capture, Result: remoteedit.Result{Changed: true}})
	if cmd != nil || !m.canSave || !m.changed || m.busy {
		t.Fatalf("capture did not wait for consent: %+v", m)
	}
	m, cmd = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if cmd != nil || m.confirm != "leave" {
		t.Fatal("escape did not ask how to retain edits")
	}
	m, cmd = m.Update(key("n"))
	if cmd != nil || m.confirm != "" {
		t.Fatal("declining exit did not retain session")
	}
	m, cmd = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil || !m.editing || m.kind != process {
		t.Fatal("Enter must edit again, never upload")
	}
}

func TestEditorErrorAndUnknownOutcomeNeverOfferUpload(t *testing.T) {
	for _, kind := range []operation{process, save} {
		m := New(remoteedit.Request{}, nil, nil, nil, 80, 24)
		m.kind, m.seq, m.canSave, m.ready = kind, 1, true, true
		m, cmd := m.Update(MsgResult{Session: m.session, ID: 1, kind: kind, Result: remoteedit.Result{
			Err: errors.New("failed"), Unknown: kind == save,
		}})
		if cmd != nil || m.canSave || m.err == nil {
			t.Fatal("failure did not preserve a non-uploadable session")
		}
		m, cmd = m.Update(key("u"))
		if cmd != nil {
			t.Fatal("failure allowed upload")
		}
		if kind == save {
			m, cmd = m.Update(key("e"))
			if cmd != nil || !m.unknown {
				t.Fatal("unknown candidate was replaced before checking its outcome")
			}
		}
	}
}

func TestStaleResultCannotReplaceCurrentOperation(t *testing.T) {
	m := New(remoteedit.Request{}, nil, nil, nil, 80, 24)
	m.seq, m.kind, m.busy = 4, save, true
	m, _ = m.Update(MsgResult{Session: m.session, ID: 3, kind: save, Result: remoteedit.Result{Saved: true}})
	if !m.busy || m.saved || m.seq != 4 {
		t.Fatal("stale result changed current session")
	}
}

func TestDiscardRequiresSeparateConfirmation(t *testing.T) {
	m := New(remoteedit.Request{}, nil, nil, nil, 80, 24)
	m, cmd := m.Update(key("d"))
	if cmd != nil || m.confirm != "discard" {
		t.Fatal("discard did not ask for confirmation")
	}
	m, cmd = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd != nil {
		t.Fatal("Enter consented to discard")
	}
	m, cmd = m.Update(key("y"))
	if cmd == nil || !m.busy || m.kind != finish {
		t.Fatal("explicit discard did not close session")
	}
	result := cmd().(MsgClosed)
	if !m.AcceptsClosed(result) {
		t.Fatal("close result identity does not match")
	}
}

func TestViewClipsAndEscapesUntrustedPaths(t *testing.T) {
	for _, height := range []int{1, 3, 12, 24} {
		m := New(remoteedit.Request{RemotePath: "/file\x1b[31m\n"}, nil, nil, nil, 20, height)
		view := m.View()
		if len(strings.Split(view, "\n")) != height {
			t.Fatalf("view height does not match %d", height)
		}
		for _, line := range strings.Split(view, "\n") {
			if ansi.StringWidth(line) > 20 {
				t.Fatalf("line exceeds viewport: %q", line)
			}
		}
		if strings.Contains(view, "\x1b[31m") {
			t.Fatal("path injected terminal control")
		}
	}
}
