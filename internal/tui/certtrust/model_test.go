package certtrust

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestPromptRejectsByDefaultAndOnEscape(t *testing.T) {
	model := New(testChallenge(), 100, 30)
	updated, cmd := model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil {
		t.Fatal("Enter did not produce a decision")
	}
	msg := cmd().(MsgDecision)
	if msg.Decision != Reject {
		t.Fatalf("default decision = %v, want Reject", msg.Decision)
	}

	updated, cmd = updated.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if cmd().(MsgDecision).Decision != Reject {
		t.Fatal("Escape did not reject")
	}
}

func TestPromptSelectsSessionTrust(t *testing.T) {
	model := New(testChallenge(), 100, 30)
	model, _ = model.Update(tea.KeyMsg{Type: tea.KeyTab})
	_, cmd := model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd().(MsgDecision).Decision != TrustSession {
		t.Fatal("Tab did not select session trust")
	}
}

func TestViewContainsFullFingerprintAndWarning(t *testing.T) {
	model := New(testChallenge(), 110, 35)
	view := ansi.Strip(model.Overlay(strings.Repeat("background\n", 35)))
	if !strings.Contains(strings.Join(model.detailLines(), ""), model.Challenge.Fingerprint) {
		t.Fatalf("view omitted the certificate fingerprint: %q", view)
	}
	if !strings.Contains(view, "Encryption alone does not confirm") {
		t.Fatal("view omitted the identity warning")
	}
}

func testChallenge() tlstrust.Challenge {
	endpoint, _ := tlstrust.NormalizeEndpoint("ftps", "server.example", 21)
	return tlstrust.Challenge{
		Endpoint:    endpoint,
		Fingerprint: "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA",
		Problems:    []tlstrust.Problem{tlstrust.ProblemUnknownAuthority},
		Subject:     "CN=server.example",
		Issuer:      "CN=Development CA",
		DNSNames:    []string{"server.example"},
	}
}
