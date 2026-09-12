package tui

import (
	"errors"
	"fmt"
	"slices"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/certtrust"
	"github.com/WariKoda/drift/internal/tui/diffview"
	tea "github.com/charmbracelet/bubbletea"
)

type trustOperationKind int

const (
	trustOperationHostTest trustOperationKind = iota
	trustOperationRemoteBrowse
	trustOperationDiffLoad
	trustOperationNoRetry
)

type pendingTrustOperation struct {
	kind      trustOperationKind
	host      config.Host
	challenge tlstrust.Challenge
}

type msgTrustSaved struct {
	id        uint64
	challenge tlstrust.Challenge
	err       error
}

func certificateChallenge(err error) (tlstrust.Challenge, bool) {
	var verificationErr *tlstrust.VerificationError
	if !errors.As(err, &verificationErr) {
		return tlstrust.Challenge{}, false
	}
	return verificationErr.Challenge, true
}

func (a *App) openCertificatePrompt(kind trustOperationKind, host config.Host, err error) bool {
	if a.certPrompt != nil {
		return false
	}
	challenge, ok := certificateChallenge(err)
	if !ok {
		return false
	}
	a.finishNetworkActivity(a.activity)
	prompt := certtrust.New(challenge, a.state.TermWidth, a.state.TermHeight)
	a.certPrompt = &prompt
	a.pendingTrust = &pendingTrustOperation{kind: kind, host: host, challenge: challenge}
	a.globalError = ""
	log.Info("FTPS certificate trust requested", "host", host.Name, "endpoint", challenge.Endpoint.Address(), "fingerprint", challenge.Fingerprint)
	return true
}

func (a *App) openDiffCertificatePrompt(err error) (tea.Cmd, bool) {
	if a.state.SelectedHost == nil {
		return nil, false
	}
	if !a.openCertificatePrompt(trustOperationNoRetry, *a.state.SelectedHost, err) {
		return nil, false
	}
	a.watchConnection(nil)
	closeCmd := a.diffView.Close()
	a.state.Screen = ScreenBrowser
	a.state.SelectedHost = nil
	return closeCmd, true
}

func (a *App) handleCertificateDecision(msg certtrust.MsgDecision) (tea.Model, tea.Cmd) {
	pending := a.pendingTrust
	if pending == nil || pending.challenge.Fingerprint != msg.Challenge.Fingerprint ||
		pending.challenge.Endpoint != msg.Challenge.Endpoint || !slices.Equal(pending.challenge.Problems, msg.Challenge.Problems) {
		a.closeCertificatePrompt()
		return *a, nil
	}
	switch msg.Decision {
	case certtrust.Reject:
		log.Info("FTPS certificate rejected", "host", pending.host.Name, "endpoint", msg.Challenge.Endpoint.Address(), "fingerprint", msg.Challenge.Fingerprint)
		a.closeCertificatePrompt()
		a.globalError = "FTPS certificate was not trusted"
		return *a, nil
	case certtrust.TrustSession:
		a.trust.TrustSession(msg.Challenge)
		log.Info("FTPS certificate trusted", "host", pending.host.Name, "endpoint", msg.Challenge.Endpoint.Address(), "fingerprint", msg.Challenge.Fingerprint, "scope", "session")
		return a.retryTrustedOperation()
	case certtrust.TrustPermanently:
		a.trustWriteSeq++
		id := a.trustWriteSeq
		a.trustSaving = true
		challenge := msg.Challenge
		return *a, func() tea.Msg {
			_, err := a.trust.TrustPermanently(challenge)
			return msgTrustSaved{id: id, challenge: challenge, err: err}
		}
	default:
		return *a, nil
	}
}

func (a *App) handleTrustSaved(msg msgTrustSaved) (tea.Model, tea.Cmd) {
	if !a.trustSaving || msg.id != a.trustWriteSeq || a.pendingTrust == nil {
		return *a, nil
	}
	a.trustSaving = false
	if msg.err != nil {
		log.Error("persist FTPS certificate trust", "endpoint", msg.challenge.Endpoint.Address(), "err", msg.err)
		a.certPrompt.SetError(msg.err)
		return *a, nil
	}
	log.Info("FTPS certificate trusted", "host", a.pendingTrust.host.Name, "endpoint", msg.challenge.Endpoint.Address(), "fingerprint", msg.challenge.Fingerprint, "scope", "permanent")
	return a.retryTrustedOperation()
}

func (a *App) retryTrustedOperation() (tea.Model, tea.Cmd) {
	pending := a.pendingTrust
	if pending == nil {
		a.closeCertificatePrompt()
		return *a, nil
	}
	a.closeCertificatePrompt()
	switch pending.kind {
	case trustOperationHostTest:
		if a.state.Screen != ScreenHostManager {
			return *a, nil
		}
		cmd := a.hostManager.RetryTest(pending.host, pending.challenge)
		_, tracker, _ := a.hostManager.Testing()
		return *a, tea.Batch(cmd, a.startNetworkActivity(activityHostTest, "Testing "+pending.host.Name+"…", tracker))
	case trustOperationRemoteBrowse:
		if a.state.Screen != ScreenBrowser {
			return *a, nil
		}
		cmd := a.browser.RetryRemote(pending.host, pending.challenge)
		_, tracker, _ := a.browser.LoadingActivity()
		return *a, tea.Batch(cmd, a.startNetworkActivity(activityRemoteLoad, "Connecting to "+pending.host.Name+"…", tracker))
	case trustOperationDiffLoad:
		if a.state.Screen != ScreenBrowser {
			return *a, nil
		}
		tracker := diffview.NewLoadProgressTracker()
		cmd := diffview.LoadCmdWithOptions(a.beginDiffRequest(), pending.host,
			a.state.Selection, a.state.RemoteSelection, a.state.Config, nil, tracker, a.trust, &pending.challenge, a.state.ScopeOptions)
		return *a, tea.Batch(cmd, a.startNetworkActivity(activityDiffLoad, "Loading diffs…", tracker))
	case trustOperationNoRetry:
		cmd := a.browser.RetryRemote(pending.host, pending.challenge)
		_, tracker, _ := a.browser.LoadingActivity()
		a.globalError = "Certificate trusted. Review the diff again before retrying the transfer."
		return *a, tea.Batch(cmd, a.startNetworkActivity(activityRemoteLoad, "Reconnecting to "+pending.host.Name+"…", tracker))
	default:
		a.globalError = fmt.Sprintf("Certificate trusted for %s", pending.host.Name)
		return *a, nil
	}
}

func (a *App) closeConnectionsFor(host config.Host) tea.Cmd {
	target, err := tlstrust.NormalizeEndpoint(host.Protocol, host.Hostname, host.Port)
	if err != nil {
		return nil
	}
	remoteHost, ok := a.browser.RemoteHost()
	if !ok {
		return nil
	}
	current, err := tlstrust.NormalizeEndpoint(remoteHost.Protocol, remoteHost.Hostname, remoteHost.Port)
	if err == nil && current == target {
		a.watchConnection(nil)
		return a.browser.CloseRemote()
	}
	return nil
}

func (a *App) closeCertificatePrompt() {
	a.certPrompt = nil
	a.pendingTrust = nil
	a.trustSaving = false
}
