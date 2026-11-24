package tui

import (
	"crypto/ed25519"
	"crypto/rand"
	"errors"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/ssh"
	"github.com/WariKoda/drift/internal/sshtest"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/hostkeytrust"
	"github.com/WariKoda/drift/internal/tui/hostselector"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
	gossh "golang.org/x/crypto/ssh"
)

// The first connection to an SSH host used to store its key without a
// question. Now the browser stops, shows the key and connects only after the
// user approved it.
func TestBrowserAsksAboutUnknownSSHHostKeyAndConnectsAfterApproval(t *testing.T) {
	for _, decision := range []hostkeytrust.Decision{hostkeytrust.TrustSession, hostkeytrust.TrustPermanently} {
		t.Run(map[hostkeytrust.Decision]string{hostkeytrust.TrustSession: "session", hostkeytrust.TrustPermanently: "permanent"}[decision], func(t *testing.T) {
			home := t.TempDir()
			t.Setenv("HOME", home)
			t.Setenv("XDG_CONFIG_HOME", t.TempDir())
			remoteDir := t.TempDir()
			if err := os.WriteFile(filepath.Join(remoteDir, "index.php"), []byte("<?php"), 0o600); err != nil {
				t.Fatal(err)
			}
			server := sshtest.Start(t, remoteDir)
			host := server.Host()
			project := t.TempDir()
			cfg := &config.MergedConfig{ProjectRoot: project, Hosts: map[string]config.Host{host.Name: host}}
			app, err := New(project, cfg, nil, nil, ScreenBrowser, false, nil)
			if err != nil {
				t.Fatal(err)
			}
			a := &app
			t.Cleanup(a.Close)
			a.state.HostSelectorPurpose = HostSelectorForRemoteBrowse
			applyEditorMessage(t, a, tea.WindowSizeMsg{Width: 100, Height: 30})

			cmd := applyEditorMessage(t, a, hostselector.MsgHostChosen{Host: host})
			failed := runEditorCommand(t, cmd).(browser.MsgRemoteLoaded)
			var unknown *ssh.UnknownHostKeyError
			if !errors.As(failed.Err, &unknown) {
				t.Fatalf("first connection: error = %v, want an unknown host key error", failed.Err)
			}
			applyEditorMessage(t, a, failed)
			if a.hostKeyPrompt == nil || a.pendingTrust == nil || a.pendingTrust.kind != trustOperationRemoteBrowse {
				t.Fatal("the unknown host key did not open its prompt")
			}
			if !a.hostKeyPrompt.Challenge.Equal(server.Challenge()) {
				t.Fatal("the prompt shows another key than the server's")
			}
			if a.globalError != "" || !strings.Contains(ansi.Strip(a.View()), "Unknown SSH host key") {
				t.Fatalf("prompt not visible; error line %q", a.globalError)
			}
			if tries := server.AuthAttempts(); tries != 0 {
				t.Fatalf("server received %d password(s) before the decision", tries)
			}
			knownHosts := filepath.Join(home, ".ssh", "known_hosts")
			if _, err := os.Stat(knownHosts); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("known_hosts exists before the decision: %v", err)
			}

			cmd = applyEditorMessage(t, a, hostkeytrust.MsgDecision{Decision: decision, Challenge: a.hostKeyPrompt.Challenge})
			if decision == hostkeytrust.TrustPermanently {
				if a.hostKeyPrompt == nil || !a.trustSaving {
					t.Fatal("the prompt closed before the key was saved")
				}
				cmd = applyEditorMessage(t, a, runEditorCommand(t, cmd))
			}
			if a.hostKeyPrompt != nil || a.pendingTrust != nil {
				t.Fatal("the prompt stayed open after approval")
			}
			connected := runEditorCommand(t, cmd).(browser.MsgRemoteLoaded)
			if connected.Err != nil || connected.Conn == nil {
				t.Fatalf("connection after approval: %v", connected.Err)
			}
			applyEditorMessage(t, a, connected)
			if a.browser.Connection() != connected.Conn {
				t.Fatal("the browser did not take the approved connection")
			}

			data, err := os.ReadFile(knownHosts)
			switch decision {
			case hostkeytrust.TrustPermanently:
				if err != nil || !strings.Contains(string(data), server.Challenge().Address) {
					t.Fatalf("known_hosts = %q, %v; want the approved host", data, err)
				}
			case hostkeytrust.TrustSession:
				if !errors.Is(err, os.ErrNotExist) {
					t.Fatalf("session trust wrote known_hosts: %q, %v", data, err)
				}
			}
		})
	}
}

func TestHostKeyPromptRejectsAndTrustsNothing(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	app, err := New(t.TempDir(), &config.MergedConfig{}, nil, nil, ScreenBrowser, false, nil)
	if err != nil {
		t.Fatal(err)
	}
	challenge := appTestHostKey(t, "server.example")
	host := config.Host{Name: "staging", Hostname: "server.example"}
	if !app.openTrustPrompt(trustOperationHostTest, host, &ssh.UnknownHostKeyError{Challenge: challenge}) {
		t.Fatal("unknown host key did not open a prompt")
	}
	if app.openTrustPrompt(trustOperationHostTest, host, &ssh.UnknownHostKeyError{Challenge: appTestHostKey(t, "other.example")}) {
		t.Fatal("a second prompt replaced the open one")
	}

	// A decision for another key than the one on screen approves nothing.
	model, cmd := app.Update(hostkeytrust.MsgDecision{Decision: hostkeytrust.TrustSession, Challenge: appTestHostKey(t, "server.example")})
	stale := model.(App)
	if cmd != nil || stale.hostKeyPrompt != nil || hostKeyTrusted(t, &stale, challenge) {
		t.Fatal("a stale decision was applied")
	}

	app.openTrustPrompt(trustOperationHostTest, host, &ssh.UnknownHostKeyError{Challenge: challenge})
	model, cmd = app.Update(tea.KeyMsg{Type: tea.KeyEnter})
	next := model.(App)
	if cmd == nil {
		t.Fatal("Enter produced no decision")
	}
	model, _ = next.Update(cmd())
	next = model.(App)
	if next.hostKeyPrompt != nil || next.globalError != "SSH host key was not trusted" {
		t.Fatalf("after the default choice: prompt open = %v, error %q", next.hostKeyPrompt != nil, next.globalError)
	}
	if hostKeyTrusted(t, &next, challenge) {
		t.Fatal("rejecting trusted the key")
	}
	if _, err := os.Stat(filepath.Join(home, ".ssh")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("rejecting wrote to ~/.ssh: %v", err)
	}
}

func TestHostKeyPromptKeepsOpenWhenSavingFails(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	// ~/.ssh is a file, so known_hosts cannot be written.
	if err := os.WriteFile(filepath.Join(home, ".ssh"), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	app, err := New(t.TempDir(), &config.MergedConfig{}, nil, nil, ScreenBrowser, false, nil)
	if err != nil {
		t.Fatal(err)
	}
	challenge := appTestHostKey(t, "server.example")
	app.openTrustPrompt(trustOperationHostTest, config.Host{Name: "staging"}, &ssh.UnknownHostKeyError{Challenge: challenge})

	model, cmd := app.Update(hostkeytrust.MsgDecision{Decision: hostkeytrust.TrustPermanently, Challenge: challenge})
	if cmd == nil {
		t.Fatal("permanent trust did not start saving")
	}
	next := model.(App)
	model, retry := next.Update(cmd())
	next = model.(App)
	if retry != nil || next.hostKeyPrompt == nil || next.hostKeyPrompt.Err == "" {
		t.Fatal("a failed save did not keep the prompt open with an error")
	}
	if hostKeyTrusted(t, &next, challenge) {
		t.Fatal("a failed save fell back to session trust")
	}
}

// hostKeyTrusted reports whether the session accepts the key. It looks at an
// empty home directory, so known_hosts plays no part in the answer.
func hostKeyTrusted(t *testing.T, a *App, challenge ssh.HostKeyChallenge) bool {
	t.Helper()
	t.Setenv("HOME", t.TempDir())
	callback, _, err := ssh.HostKeyCallback(challenge.Address, 22, a.trust.HostKeys())
	if err != nil {
		t.Fatal(err)
	}
	return callback(challenge.Address+":22", &net.TCPAddr{IP: net.IPv4(192, 0, 2, 1), Port: 22}, challenge.Key) == nil
}

func appTestHostKey(t *testing.T, address string) ssh.HostKeyChallenge {
	t.Helper()
	_, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(private)
	if err != nil {
		t.Fatal(err)
	}
	return ssh.HostKeyChallenge{Address: address, Key: signer.PublicKey()}
}
