package certtrust

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/loading"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Overlay dims base and places the trust prompt in the terminal center.
func (m Model) Overlay(base string) string {
	return loading.OverlayCentered(base, m.modal(), m.Width, m.Height)
}

func (m Model) modal() string {
	contentWidth := min(76, max(24, m.Width-8))
	lines := m.detailLines()
	viewportHeight := min(len(lines), m.viewportHeight())
	start := min(m.Offset, max(0, len(lines)-viewportHeight))
	end := min(len(lines), start+viewportHeight)
	visible := append([]string(nil), lines[start:end]...)
	if start > 0 && len(visible) > 0 {
		visible[0] = styles.Muted.Render("↑ more")
	}
	if end < len(lines) && len(visible) > 0 {
		visible[len(visible)-1] = styles.Muted.Render("↓ more")
	}

	buttons := []string{
		m.button("Reject", Reject),
		m.button("Trust for this session", TrustSession),
		m.button("Trust permanently", TrustPermanently),
	}
	body := []string{styles.Header.Render("Certificate verification failed"), ""}
	body = append(body, visible...)
	if m.Err != "" {
		body = append(body, "", styles.Err.Render("Could not save trust: "+truncate(m.Err, contentWidth)))
	}
	body = append(body, "", strings.Join(buttons, "  "),
		styles.Muted.Render("[Tab/←/→] select  [Enter] confirm  [Esc] reject  [↑/↓] scroll"))
	return styles.LoadingBox.Width(contentWidth).Render(strings.Join(body, "\n"))
}

func (m Model) detailLines() []string {
	challenge := m.Challenge
	names := append([]string(nil), challenge.DNSNames...)
	names = append(names, challenge.IPAddresses...)
	if len(names) == 0 {
		names = []string{"(none)"}
	}
	problems := make([]string, len(challenge.Problems))
	for i, problem := range challenge.Problems {
		problems[i] = problemLabel(problem)
	}
	lines := []string{
		"Server:       " + challenge.Endpoint.Address(),
		"Problems:     " + strings.Join(problems, ", "),
		"Subject:      " + emptyFallback(challenge.Subject),
		"DNS/IP names: " + strings.Join(names, ", "),
		"Issuer:       " + emptyFallback(challenge.Issuer),
		"Valid from:   " + challenge.NotBefore.UTC().Format("2006-01-02 15:04 MST"),
		"Valid until:  " + challenge.NotAfter.UTC().Format("2006-01-02 15:04 MST"),
		"SHA-256:      " + challenge.Fingerprint,
	}
	if challenge.PreviousSHA256 != "" {
		lines = append(lines, "Previous:     "+challenge.PreviousSHA256)
	}
	lines = append(lines, "",
		"Verify this fingerprint through a trusted channel.",
		"Encryption alone does not confirm the server's identity.")
	return wrapLines(lines, min(72, max(20, m.Width-12)))
}

func (m Model) button(label string, decision Decision) string {
	text := "[" + label + "]"
	if m.Selection == decision {
		return styles.Marked.Render(text)
	}
	return styles.Key.Render(text)
}

func problemLabel(problem tlstrust.Problem) string {
	switch problem {
	case tlstrust.ProblemUnknownAuthority:
		return "unknown certificate authority"
	case tlstrust.ProblemHostnameMismatch:
		return "hostname mismatch"
	case tlstrust.ProblemExpired:
		return "certificate expired"
	case tlstrust.ProblemNotYetValid:
		return "certificate not yet valid"
	case tlstrust.ProblemCertificateChange:
		return "certificate changed before reconnect"
	default:
		return string(problem)
	}
}

func wrapLines(lines []string, width int) []string {
	var wrapped []string
	for _, line := range lines {
		if line == "" {
			wrapped = append(wrapped, "")
			continue
		}
		wrapped = append(wrapped, strings.Split(ansi.Hardwrap(line, width, true), "\n")...)
	}
	return wrapped
}

func emptyFallback(value string) string {
	if value == "" {
		return "(not provided)"
	}
	return value
}

func truncate(value string, width int) string {
	if lipgloss.Width(value) <= width {
		return value
	}
	return ansi.Truncate(value, max(1, width-1), "") + "…"
}
