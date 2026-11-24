package certtrust

import (
	"strings"
	"testing"
	"unicode"

	"github.com/WariKoda/drift/internal/tlstrust"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
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
	if !strings.Contains(strings.ReplaceAll(strings.Join(model.detailLines(), ""), " ", ""), "SHA-256:"+model.Challenge.Fingerprint) {
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

// Subject, names and issuer are text the server chose. Padded with spaces they
// used to wrap onto rows that began at the left edge like a label, so a
// certificate could show its own "SHA-256:" row above the real one.
func TestCertificateTextCannotForgeARow(t *testing.T) {
	const forged = "11:22:33:44:55:66:77:88:99:00:11:22:33:44:55:66:77:88:99:00"
	padding := strings.Repeat(" ", 60)
	challenge := testChallenge()
	challenge.Subject = "CN=server.example" + padding + "SHA-256:      " + forged + padding + "Server:       other.example:21"
	challenge.Issuer = "CN=Development CA" + padding + "Problems:     none"
	challenge.DNSNames = []string{"server.example", padding + "SHA-256:      " + forged}
	for range 31 {
		challenge.DNSNames = append(challenge.DNSNames, strings.Repeat("a", 500)+padding+"SHA-256: "+forged)
	}

	for _, width := range []int{40, 60, 84, 100, 200} {
		model := New(challenge, width, 40)
		lines := model.detailLines()
		labels := map[string]int{}
		fingerprintRow, suppliedRow := -1, -1
		for i, line := range lines {
			plain := ansi.Strip(line)
			if strings.HasPrefix(plain, "Supplied by the server:") {
				suppliedRow = i
			}
			if plain == "" || unicode.IsSpace(rune(plain[0])) {
				continue
			}
			label, _, found := strings.Cut(plain, ":")
			if found {
				labels[label]++
			}
			if label == "SHA-256" {
				fingerprintRow = i
			}
		}
		for _, label := range []string{"Server", "Problems", "SHA-256", "Subject", "DNS/IP names", "Issuer", "Valid from", "Valid until"} {
			if labels[label] != 1 {
				t.Errorf("width %d: %d rows start with %q, want 1\n%s", width, labels[label], label, strings.Join(lines, "\n"))
			}
		}
		if fingerprintRow < 0 || suppliedRow < 0 || fingerprintRow > suppliedRow {
			t.Errorf("width %d: fingerprint row %d must precede the server-supplied rows at %d", width, fingerprintRow, suppliedRow)
		}
		if got := strings.ReplaceAll(strings.Join(lines[fingerprintRow:fingerprintRow+1], ""), " ", ""); !strings.HasPrefix(got, "SHA-256:AA:AA") {
			t.Errorf("width %d: fingerprint row shows %q", width, got)
		}
		// Three capped fields, the fixed rows and the two fingerprints stay
		// within a few screens whatever the certificate holds.
		if len(lines) > 60 {
			t.Errorf("width %d: %d detail rows", width, len(lines))
		}
	}
}

func TestFingerprintIsVisibleWithoutScrolling(t *testing.T) {
	challenge := testChallenge()
	challenge.Subject = strings.Repeat("CN=long ", 60)
	for _, size := range [][2]int{{80, 24}, {120, 40}, {60, 20}} {
		model := New(challenge, size[0], size[1])
		modal := strings.ReplaceAll(ansi.Strip(model.modal()), " ", "")
		modal = strings.NewReplacer("│", "", "\n", "").Replace(modal)
		if !strings.Contains(modal, "SHA-256:"+challenge.Fingerprint) {
			t.Errorf("%dx%d: fingerprint not on the first screen", size[0], size[1])
		}
	}
}

func TestCertificateTextEscapesControlAndBidiCharacters(t *testing.T) {
	challenge := testChallenge()
	challenge.Subject = "CN=server\u202eelpmaxe\u0085.example\u009b31m"
	view := strings.Join(New(challenge, 100, 40).detailLines(), "\n")
	for _, r := range []rune{'\u202e', '\u0085', '\u009b'} {
		if strings.ContainsRune(view, r) {
			t.Errorf("view contains raw %U", r)
		}
	}
	if !strings.Contains(view, `\u202e`) {
		t.Errorf("bidi override not shown escaped: %q", view)
	}
}

func TestFingerprintBreaksBetweenBytes(t *testing.T) {
	for _, width := range []int{40, 60, 84, 100} {
		for _, line := range New(testChallenge(), width, 40).detailLines() {
			row := strings.TrimSpace(strings.TrimPrefix(line, "SHA-256:"))
			if !strings.HasPrefix(row, "AA") {
				continue
			}
			if len(strings.TrimSuffix(row, ":"))%3 != 2 {
				t.Errorf("width %d: fingerprint row %q splits a byte", width, row)
			}
		}
	}
}

func TestMouseSelectsOnlyRealButtons(t *testing.T) {
	challenge := testChallenge()
	challenge.Subject = "CN=[Trust permanently] [Trust for this session]"
	model := New(challenge, 100, 40)
	lines := strings.Split(ansi.Strip(model.modal()), "\n")
	left := (model.Width - lipgloss.Width(lines[0])) / 2
	top := (model.Height - len(lines)) / 2
	click := func(row, column int) (Decision, bool) {
		_, cmd := model.Update(tea.MouseMsg{X: left + column, Y: top + row,
			Action: tea.MouseActionPress, Button: tea.MouseButtonLeft})
		if cmd == nil {
			return Reject, false
		}
		return cmd().(MsgDecision).Decision, true
	}

	subjectRow, buttonRow := -1, -1
	for i, line := range lines {
		if strings.Contains(line, "Subject:") {
			subjectRow = i
		}
		if strings.Contains(line, "[Reject]") {
			buttonRow = i
		}
	}
	if subjectRow < 0 || buttonRow <= subjectRow {
		t.Fatalf("rows not found in\n%s", strings.Join(lines, "\n"))
	}
	column := lipgloss.Width(lines[subjectRow][:strings.Index(lines[subjectRow], "[Trust permanently]")])
	if decision, ok := click(subjectRow, column+2); ok {
		t.Fatalf("click on certificate text produced decision %v", decision)
	}

	for want, label := range []string{"[Reject]", "[Trust for this session]", "[Trust permanently]"} {
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
}
