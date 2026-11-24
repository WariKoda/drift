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
		styles.KeyHints("[Tab/←/→] select  [Enter] confirm  [Esc] reject  [↑/↓] scroll", styles.Muted))
	return styles.LoadingBox.Width(contentWidth).Render(strings.Join(body, "\n"))
}

// labelWidth is the column where every value starts. Continuation rows are
// indented to it, so wrapped text can never begin where a label begins.
const labelWidth = 14

// maxCertificateRows caps each field the server supplies. Longer values end in
// an ellipsis; the fingerprint, not these fields, identifies the certificate.
const maxCertificateRows = 5

// detailLines puts everything drift computed itself above the text taken from
// the certificate. Subject, names and issuer are whatever the server chose to
// write there, and a prompt that printed them first, wrapped to the left edge,
// let a certificate draw its own "SHA-256:" row above the real one.
func (m Model) detailLines() []string {
	challenge := m.Challenge
	width := min(72, max(labelWidth+6, m.Width-12))
	problems := make([]string, len(challenge.Problems))
	for i, problem := range challenge.Problems {
		problems[i] = problemLabel(problem)
	}
	valueWidth := width - labelWidth
	var lines []string
	add := func(label string, rows []string) {
		lines = append(lines, labelled(label, rows)...)
	}
	add("Server:", wrapValue(challenge.Endpoint.Address(), valueWidth, 0))
	add("Problems:", wrapValue(strings.Join(problems, ", "), valueWidth, 0))
	add("SHA-256:", fingerprintRows(challenge.Fingerprint, valueWidth))
	if challenge.PreviousSHA256 != "" {
		add("Previous:", fingerprintRows(challenge.PreviousSHA256, valueWidth))
	}
	add("Valid from:", []string{challenge.NotBefore.UTC().Format("2006-01-02 15:04 MST")})
	add("Valid until:", []string{challenge.NotAfter.UTC().Format("2006-01-02 15:04 MST")})
	lines = append(lines, "")
	lines = append(lines, wrapLines([]string{
		"Verify this fingerprint through a trusted channel.",
		"Encryption alone does not confirm the server's identity.",
	}, width)...)

	names := append([]string(nil), challenge.DNSNames...)
	names = append(names, challenge.IPAddresses...)
	for i, name := range names {
		names[i] = certificateText(name)
	}
	if len(names) == 0 {
		names = []string{"(none)"}
	}
	lines = append(lines, "", styles.Muted.Render("Supplied by the server:"))
	add("Subject:", wrapValue(emptyFallback(certificateText(challenge.Subject)), valueWidth, maxCertificateRows))
	add("DNS/IP names:", wrapValue(strings.Join(names, ", "), valueWidth, maxCertificateRows))
	add("Issuer:", wrapValue(emptyFallback(certificateText(challenge.Issuer)), valueWidth, maxCertificateRows))
	return lines
}

// labelled puts label before the first row and indents the others.
func labelled(label string, rows []string) []string {
	out := make([]string, len(rows))
	for i, row := range rows {
		if i == 0 {
			out[i] = label + strings.Repeat(" ", max(1, labelWidth-len(label))) + row
		} else {
			out[i] = strings.Repeat(" ", labelWidth) + row
		}
	}
	return out
}

// wrapValue breaks value into rows of at most width cells. maxRows of 0 means
// no limit; a longer value ends in an ellipsis.
func wrapValue(value string, width, maxRows int) []string {
	rows := strings.Split(ansi.Hardwrap(value, width, false), "\n")
	if maxRows > 0 && len(rows) > maxRows {
		rows = rows[:maxRows]
		rows[maxRows-1] = ansi.Truncate(rows[maxRows-1], width-1, "") + "…"
	}
	return rows
}

// fingerprintRows breaks a colon-separated fingerprint between bytes, so the
// rows can be compared pair by pair. A trailing colon marks a row that continues.
func fingerprintRows(fingerprint string, width int) []string {
	pairs := strings.Split(fingerprint, ":")
	perRow := max(1, width/3)
	var rows []string
	for len(pairs) > perRow {
		rows = append(rows, strings.Join(pairs[:perRow], ":")+":")
		pairs = pairs[perRow:]
	}
	return append(rows, strings.Join(pairs, ":"))
}

// certificateText prepares a string from the certificate for display: runs of
// whitespace collapse to one space, and control and bidirectional characters
// are shown escaped.
func certificateText(value string) string {
	return styles.SafeText(strings.Join(strings.Fields(value), " "))
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
