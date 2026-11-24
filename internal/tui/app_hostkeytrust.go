package tui

import (
	"errors"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/ssh"
	"github.com/WariKoda/drift/internal/tui/hostkeytrust"
	tea "github.com/charmbracelet/bubbletea"
)

type msgHostKeySaved struct {
	id        uint64
	challenge ssh.HostKeyChallenge
	err       error
}

// openHostKeyPrompt asks about the key of an SSH host that known_hosts does
// not list. The connection that reported it has already ended, before
// authentication; approval starts a new one.
func (a *App) openHostKeyPrompt(kind trustOperationKind, host config.Host, err error) bool {
	if a.certPrompt != nil || a.hostKeyPrompt != nil {
		return false
	}
	var unknown *ssh.UnknownHostKeyError
	if !errors.As(err, &unknown) {
		return false
	}
	challenge := unknown.Challenge
	a.finishNetworkActivity(a.activity)
	prompt := hostkeytrust.New(challenge, a.state.TermWidth, a.state.TermHeight)
	a.hostKeyPrompt = &prompt
	a.pendingTrust = &pendingTrustOperation{kind: kind, host: host, hostKey: &challenge}
	a.globalError = ""
	log.Info("SSH host key trust requested", "host", host.Name, "address", challenge.Address, "fingerprint", challenge.Fingerprint())
	return true
}

func (a *App) handleHostKeyDecision(msg hostkeytrust.MsgDecision) (tea.Model, tea.Cmd) {
	pending := a.pendingTrust
	if pending == nil || pending.hostKey == nil || !pending.hostKey.Equal(msg.Challenge) {
		a.closeTrustPrompt()
		return *a, nil
	}
	challenge := *pending.hostKey
	switch msg.Decision {
	case hostkeytrust.Reject:
		log.Info("SSH host key rejected", "host", pending.host.Name, "address", challenge.Address, "fingerprint", challenge.Fingerprint())
		a.closeTrustPrompt()
		a.globalError = "SSH host key was not trusted"
		return *a, nil
	case hostkeytrust.TrustSession:
		a.trust.HostKeys().Trust(challenge)
		log.Info("SSH host key trusted", "host", pending.host.Name, "address", challenge.Address, "fingerprint", challenge.Fingerprint(), "scope", "session")
		return a.retryTrustedOperation()
	case hostkeytrust.TrustPermanently:
		a.trustWriteSeq++
		id := a.trustWriteSeq
		a.trustSaving = true
		return *a, func() tea.Msg {
			return msgHostKeySaved{id: id, challenge: challenge, err: ssh.SaveHostKey(challenge)}
		}
	default:
		return *a, nil
	}
}

func (a *App) handleHostKeySaved(msg msgHostKeySaved) (tea.Model, tea.Cmd) {
	if !a.trustSaving || msg.id != a.trustWriteSeq || a.pendingTrust == nil || a.hostKeyPrompt == nil {
		return *a, nil
	}
	a.trustSaving = false
	if msg.err != nil {
		log.Error("save SSH host key", "address", msg.challenge.Address, "err", msg.err)
		a.hostKeyPrompt.SetError(msg.err)
		return *a, nil
	}
	log.Info("SSH host key trusted", "host", a.pendingTrust.host.Name, "address", msg.challenge.Address, "fingerprint", msg.challenge.Fingerprint(), "scope", "permanent")
	return a.retryTrustedOperation()
}
