package hostmanager

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
)

func TestResetCertificateTrustRequiresConfirmation(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	host := config.Host{Name: "staging", Hostname: "server.example", Port: 21, Protocol: "ftps"}
	cfg := &config.MergedConfig{GlobalHosts: []config.Host{host}}
	manager := tlstrust.NewManager()
	endpoint, err := tlstrust.NormalizeEndpoint("ftps", host.Hostname, host.Port)
	if err != nil {
		t.Fatal(err)
	}
	challenge := tlstrust.Challenge{
		Endpoint:    endpoint,
		Fingerprint: "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA",
		Problems:    []tlstrust.Problem{tlstrust.ProblemUnknownAuthority},
	}
	manager.TrustSession(challenge)
	model := New(cfg, 120, 30)
	model.SetTrustManager(manager)

	model, cmd := model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'r'}})
	if cmd != nil || !model.confirmReset {
		t.Fatal("reset did not enter confirmation state")
	}
	if !strings.Contains(model.View(), "endpoint may be shared") {
		t.Fatal("reset warning is missing")
	}
	model, cmd = model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil {
		t.Fatal("confirmed reset did not start a command")
	}
	result := cmd().(MsgTrustReset)
	if result.Err != nil {
		t.Fatalf("reset trust: %v", result.Err)
	}
	model, _ = model.Update(result)
	trusted, err := manager.HasTrust(endpoint)
	if err != nil {
		t.Fatal(err)
	}
	if trusted {
		t.Fatal("session trust remained after reset")
	}
	if !strings.Contains(model.View(), "certificate trust reset") {
		t.Fatal("reset result is not visible")
	}
}
