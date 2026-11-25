package hostkeytrust

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/WariKoda/drift/internal/tui/loading"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

var buttonLabels = []string{"[Reject]", "[Trust for this session]", "[Trust permanently]"}

// Overlay dims base and places the trust prompt in the terminal center.
func (m Model) Overlay(base string) string {
	return loading.OverlayCentered(base, m.modal(), m.Width, m.Height)
}

func (m Model) modal() string {
	contentWidth := min(76, max(24, m.Width-8))
	body := []string{styles.Header.Render("Unknown SSH host key"), ""}
	body = append(body, m.detailLines()...)
	if m.Err != "" {
		body = append(body, "", styles.Err.Render("Could not save the key: "+truncate(styles.SafeText(m.Err), contentWidth)))
	}
	buttons := make([]string, len(buttonLabels))
	for i, label := range buttonLabels {
		if m.Selection == Decision(i) {
			buttons[i] = styles.Marked.Render(label)
		} else {
			buttons[i] = styles.Key.Render(label)
		}
	}
	body = append(body, "", strings.Join(buttons, "  "),
		styles.KeyHints("[Tab/←/→] select  [Enter] confirm  [Esc] reject", styles.Muted))
	return styles.LoadingBox.Width(contentWidth).Render(strings.Join(body, "\n"))
}

// labelWidth is the column where every value starts; a wrapped value continues
// there.
const labelWidth = 14

func (m Model) detailLines() []string {
	width := min(72, max(labelWidth+6, m.Width-12))
	var lines []string
	for _, field := range [][2]string{
		{"Server:", styles.SafeText(m.Challenge.Address)},
		{"Key type:", m.Challenge.KeyType()},
		{"Fingerprint:", m.Challenge.Fingerprint()},
	} {
		rows := strings.Split(ansi.Hardwrap(field[1], width-labelWidth, false), "\n")
		for i, row := range rows {
			label := ""
			if i == 0 {
				label = field[0]
			}
			lines = append(lines, label+strings.Repeat(" ", labelWidth-len(label))+row)
		}
	}
	lines = append(lines, "")
	for _, hint := range []string{
		"Verify this fingerprint through a trusted channel.",
		"Permanent trust is stored in ~/.ssh/known_hosts, which ssh reads too.",
	} {
		lines = append(lines, strings.Split(ansi.Wordwrap(hint, width, ""), "\n")...)
	}
	return lines
}

func truncate(value string, width int) string {
	if lipgloss.Width(value) <= width {
		return value
	}
	return ansi.Truncate(value, max(1, width-1), "") + "…"
}
