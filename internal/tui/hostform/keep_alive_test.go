package hostform

import (
	"fmt"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

func TestKeepAliveFormSaveRoundTrip(t *testing.T) {
	for _, protocol := range []Protocol{ProtoSFTP, ProtoFTP, ProtoFTPS} {
		for _, value := range []string{"", "0", "1", "86400"} {
			t.Run(protocol.String()+"/"+value, func(t *testing.T) {
				t.Setenv("XDG_CONFIG_HOME", t.TempDir())
				m := New(config.ScopeProject, "shop", 100, 40)
				m.protocol = protocol
				m.fields[fName].SetValue("prod")
				m.fields[fHostname].SetValue("localhost")
				m.fields[fRootPath].SetValue("/srv/app")
				if value == "0" {
					m.keepAliveDisabled = true
				} else {
					m.fields[fKeepAliveInterval].SetValue(value)
				}
				updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlS})
				if cmd == nil || updated.errMsg != "" {
					t.Fatalf("save failed: %s", updated.errMsg)
				}
				saved, ok := cmd().(MsgHostSaved)
				if !ok {
					t.Fatal("save did not emit MsgHostSaved")
				}
				cfg := &config.MergedConfig{ProjectRoot: t.TempDir(), ProjectSlug: "shop"}
				if err := config.SaveProjectHost(cfg, saved.Host, ""); err != nil {
					t.Fatal(err)
				}
				loaded, err := config.Load(cfg.ProjectRoot, cfg.ProjectSlug)
				if err != nil {
					t.Fatal(err)
				}
				h := loaded.Hosts["prod"]
				edit := NewEdit(h, saved.Scope, cfg.ProjectSlug, 100, 40)
				wantText := value
				if value == "0" {
					wantText = ""
				}
				if got := edit.fields[fKeepAliveInterval].Value(); got != wantText || edit.keepAliveDisabled != (value == "0") {
					t.Fatalf("edited interval = %q, disabled = %v", got, edit.keepAliveDisabled)
				}
				edit.focusRow = len(edit.visibleRows()) - 1
				edit.applyFocus()
				updated, cmd = edit.Update(tea.KeyMsg{Type: tea.KeyEnter})
				if cmd == nil || updated.errMsg != "" {
					t.Fatalf("save from scope row failed: %s", updated.errMsg)
				}
				saved = cmd().(MsgHostSaved)
				if !saved.IsEdit || saved.OldName != "prod" || saved.Host.Protocol != protocol.String() {
					t.Fatalf("edit save lost host metadata: %+v", saved)
				}
				want := 60 * time.Second
				if value == "" {
					if saved.Host.KeepAliveInterval != nil {
						t.Fatal("blank field populated interval")
					}
				} else {
					n, err := strconv.Atoi(value)
					if err != nil {
						t.Fatal(err)
					}
					want = time.Duration(n) * time.Second
					if saved.Host.KeepAliveInterval == nil || *saved.Host.KeepAliveInterval != n {
						t.Fatalf("saved interval = %v, want %d", saved.Host.KeepAliveInterval, n)
					}
				}
				if saved.Host.KeepAliveDuration() != want {
					t.Fatalf("saved interval duration = %v, want %v", saved.Host.KeepAliveDuration(), want)
				}
				// Clearing an explicit setting restores default without mutating the source host.
				edit.keepAliveDisabled = false
				edit.fields[fKeepAliveInterval].SetValue("")
				cleared, err := edit.toHost()
				if err != nil || cleared.KeepAliveInterval != nil {
					t.Fatalf("clearing interval failed: %+v, %v", cleared, err)
				}
				if value != "" && fmt.Sprint(*h.KeepAliveInterval) != value {
					t.Fatal("editing mutated the source host interval")
				}
			})
		}
	}
}

func TestKeepAliveDisableSwitchHidesIntervalAndPreservesInput(t *testing.T) {
	keys := []tea.KeyMsg{{Type: tea.KeyEnter}, {Type: tea.KeyRunes, Runes: []rune{' '}}, {Type: tea.KeyLeft}, {Type: tea.KeyRight}}
	for _, protocol := range []Protocol{ProtoSFTP, ProtoFTP, ProtoFTPS} {
		for _, key := range keys {
			t.Run(protocol.String()+"/"+key.String(), func(t *testing.T) {
				m := NewEdit(config.Host{Name: "prod", Hostname: "localhost", RootPath: "/app"}, config.ScopeGlobal, "", 100, 40)
				m.protocol = protocol
				m.fields[fKeepAliveInterval].SetValue("120")
				m.focusRow = slices.Index(m.visibleRows(), fKeepAliveDisabled)
				m.applyFocus()
				m, cmd := m.Update(key)
				if cmd != nil || !m.keepAliveDisabled || slices.Contains(m.visibleRows(), fKeepAliveInterval) {
					t.Fatal("switch did not disable and hide the interval")
				}
				if strings.Contains(m.View(), "Keep-alive interval (seconds)") || !strings.Contains(m.View(), "Disable keep-alive") {
					t.Fatal("disabled form displays interval or hides switch")
				}
				if m.visibleRows()[m.focusRow] != fKeepAliveDisabled || m.fields[fKeepAliveInterval].Focused {
					t.Fatal("toggle lost focus or left hidden field focused")
				}
				h, err := m.toHost()
				if err != nil || h.KeepAliveInterval == nil || *h.KeepAliveInterval != 0 {
					t.Fatalf("disabled setting not saved as zero: %+v, %v", h, err)
				}
				m, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
				if m.visibleRows()[m.focusRow] != fRootPath {
					t.Fatal("Tab did not skip hidden interval")
				}
				m, _ = m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
				m, _ = m.Update(key)
				if m.keepAliveDisabled || !slices.Contains(m.visibleRows(), fKeepAliveInterval) || m.fields[fKeepAliveInterval].Value() != "120" {
					t.Fatal("reenabling did not restore interval")
				}
				h, err = m.toHost()
				if err != nil || h.KeepAliveInterval == nil || *h.KeepAliveInterval != 120 {
					t.Fatalf("restored interval not saved: %+v, %v", h, err)
				}
			})
		}
	}
}

func TestKeepAliveDisabledIgnoresHiddenInvalidInput(t *testing.T) {
	m := NewEdit(config.Host{Name: "prod", Hostname: "localhost", RootPath: "/app"}, config.ScopeGlobal, "", 100, 40)
	m.fields[fKeepAliveInterval].SetValue("invalid")
	m.keepAliveDisabled = true
	if h, err := m.toHost(); err != nil || h.KeepAliveInterval == nil || *h.KeepAliveInterval != 0 {
		t.Fatalf("hidden input blocked disabling: %+v, %v", h, err)
	}
}

func TestKeepAliveFormRejectsInvalidInput(t *testing.T) {
	for _, value := range []string{"0", "-1", "86401", "864000", "1.5", "abc", " ", "9999999999999999999999999"} {
		t.Run(value, func(t *testing.T) {
			m := NewEdit(config.Host{Name: "prod", Hostname: "localhost", RootPath: "/app"}, config.ScopeGlobal, "", 100, 40)
			m.focusRow = slices.Index(m.visibleRows(), fKeepAliveInterval)
			m.applyFocus()
			m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(value)})
			updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlS})
			if cmd != nil || !strings.Contains(updated.errMsg, "Keep-alive interval must be an integer between 1 and 86400 seconds") {
				t.Fatalf("invalid input produced cmd=%v, error=%q", cmd != nil, updated.errMsg)
			}
			if !strings.Contains(updated.View(), updated.errMsg) {
				t.Fatal("validation error is not visible")
			}
		})
	}
}

func TestKeepAliveFormFocusAndProtocolSwitch(t *testing.T) {
	m := New(config.ScopeGlobal, "", 100, 40)
	if m.fields[fKeepAliveInterval].Value() != "" {
		t.Fatal("new form does not start with blank interval")
	}
	for _, want := range []Protocol{ProtoFTP, ProtoFTPS, ProtoSFTP} {
		m.focusRow = slices.Index(m.visibleRows(), fProtocol)
		m.applyFocus()
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
		if m.protocol != want {
			t.Fatalf("protocol = %v, want %v", m.protocol, want)
		}
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
		if m.visibleRows()[m.focusRow] != fKeepAliveDisabled {
			t.Fatal("Tab from protocol does not focus keep-alive switch")
		}
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
		if m.visibleRows()[m.focusRow] != fKeepAliveInterval || !m.fields[fKeepAliveInterval].Focused {
			t.Fatal("Tab from protocol does not focus keep-alive interval")
		}
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("0")})
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
		if m.visibleRows()[m.focusRow] != fRootPath || m.fields[fKeepAliveInterval].Focused {
			t.Fatal("Enter from interval does not focus root path")
		}
		m, _ = m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
		if m.visibleRows()[m.focusRow] != fKeepAliveInterval {
			t.Fatal("Shift+Tab does not return to keep-alive interval")
		}
		view := m.View()
		if !strings.Contains(view, "Keep-alive interval (seconds)") || !strings.Contains(view, "Blank uses 60 seconds.") {
			t.Fatal("interval label or English help is missing")
		}
	}
	if got := m.fields[fKeepAliveInterval].Value(); got != "000" {
		t.Fatalf("protocol switches lost interval input: %q", got)
	}
	m.SetSize(80, 24)
	if m.fields[fKeepAliveInterval].Width != 6 {
		t.Fatal("resize changed interval input width")
	}
}
