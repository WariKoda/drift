package hostkeytrust

import (
	"crypto/ed25519"
	"crypto/rand"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/ssh"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	gossh "golang.org/x/crypto/ssh"
)

func TestPromptRejectsByDefaultAndOnEscape(t *testing.T) {
	model := New(testChallenge(t), 100, 30)
	updated, cmd := model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil || cmd().(MsgDecision).Decision != Reject {
		t.Fatal("Enter on a new prompt did not reject")
	}
	if _, cmd = updated.Update(tea.KeyMsg{Type: tea.KeyEsc}); cmd().(MsgDecision).Decision != Reject {
		t.Fatal("Escape did not reject")
	}
}

func TestPromptSelectsTrustScope(t *testing.T) {
	model := New(testChallenge(t), 100, 30)
	for _, want := range []Decision{TrustSession, TrustPermanently, Reject} {
		model, _ = model.Update(tea.KeyMsg{Type: tea.KeyTab})
		_, cmd := model.Update(tea.KeyMsg{Type: tea.KeyEnter})
		decision := cmd().(MsgDecision)
		if decision.Decision != want || !decision.Challenge.Equal(model.Challenge) {
			t.Fatalf("decision = %+v, want %v for the shown key", decision, want)
		}
	}
}

func TestViewShowsAddressKeyTypeAndFullFingerprint(t *testing.T) {
	challenge := testChallenge(t)
	for _, size := range [][2]int{{110, 35}, {80, 24}, {60, 20}} {
		view := ansi.Strip(New(challenge, size[0], size[1]).Overlay(strings.Repeat(".\n", size[1])))
		// A narrow terminal wraps the fingerprint onto an indented second row.
		joined := strings.NewReplacer(" ", "", "│", "", "\n", "", ".", "").Replace(view)
		for _, want := range []string{"[examplecom]:2222", "ssh-ed25519", "Fingerprint:" + challenge.Fingerprint(), "known_hosts"} {
			if !strings.Contains(joined, strings.ReplaceAll(want, ".", "")) {
				t.Errorf("%dx%d: view lacks %q\n%s", size[0], size[1], want, view)
			}
		}
	}
}

func TestMouseSelectsButtonsAtTheirCells(t *testing.T) {
	model := New(testChallenge(t), 100, 30)
	lines := strings.Split(ansi.Strip(model.modal()), "\n")
	left := (model.Width - lipgloss.Width(lines[0])) / 2
	top := (model.Height - len(lines)) / 2
	buttonRow := -1
	for i, line := range lines {
		if strings.Contains(line, "[Reject]") {
			buttonRow = i
		}
	}
	click := func(row, column int) (Decision, bool) {
		_, cmd := model.Update(tea.MouseMsg{X: left + column, Y: top + row,
			Action: tea.MouseActionPress, Button: tea.MouseButtonLeft})
		if cmd == nil {
			return Reject, false
		}
		return cmd().(MsgDecision).Decision, true
	}
	for want, label := range buttonLabels {
		start := lipgloss.Width(lines[buttonRow][:strings.Index(lines[buttonRow], label)])
		for _, column := range []int{start, start + len(label) - 1} {
			if decision, ok := click(buttonRow, column); !ok || decision != Decision(want) {
				t.Errorf("click on %s at column %d = %v, %v", label, column, decision, ok)
			}
		}
		if _, ok := click(buttonRow, start+len(label)); ok {
			t.Errorf("click right after %s produced a decision", label)
		}
	}
	if _, ok := click(buttonRow-3, 10); ok {
		t.Error("click on the details produced a decision")
	}
}

func testChallenge(t *testing.T) ssh.HostKeyChallenge {
	t.Helper()
	_, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(private)
	if err != nil {
		t.Fatal(err)
	}
	return ssh.HostKeyChallenge{Address: "[example.com]:2222", Key: signer.PublicKey()}
}
