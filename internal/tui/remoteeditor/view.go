package remoteeditor

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/x/ansi"
)

func (m Model) View() string {
	lines := []string{
		styles.Header.Render("Edit remote file"),
		styles.File.Render("Host: " + styles.SafeText(m.host.Name)),
		styles.File.Render("Remote: " + styles.SafeText(m.path)),
		styles.Muted.Render("Working copy: " + styles.SafeText(m.file)),
		"",
		styles.File.Render(styles.SafeText(m.status)),
	}
	if m.err != nil {
		lines = append(lines, styles.Err.Render(styles.SafeText(m.err.Error())))
	}
	lines = append(lines, "", styles.Muted.Render("Saving in the editor does not upload. Local project files stay untouched."))
	footer := ""
	switch {
	case m.confirm == "discard":
		footer = "Discard the working copy? [y] discard  [n/Esc] continue"
	case m.confirm == "reload":
		footer = "Replace your changes with remote content? [y] reload  [n/Esc] continue"
	case m.confirm == "leave":
		footer = "[k] keep local copy and return  [d] discard…  [Esc] continue"
	case m.editing:
		footer = "Close the file in the editor to return here. GUI editors need a wait option."
	case m.busy:
		footer = "Working… [Esc/Ctrl+C] request cancellation; upload outcome may be unknown"
	default:
		if m.canSave && !m.unknown && !m.conflict && usable(m.conn) {
			lines = append(lines, styles.KeyHints("[u] upload confirmed snapshot", styles.Dir))
		}
		if m.ready && !m.unknown {
			lines = append(lines, styles.KeyHints("[e/Enter] edit again", styles.Dir))
		}
		if m.ready && usable(m.conn) {
			lines = append(lines, styles.KeyHints("[r] reload remote (discards changes after confirmation)", styles.Dir))
		}
		lines = append(lines, styles.KeyHints("[c] reconnect and check (never uploads)", styles.Dir))
		footer = "[k] keep local copy  [d] discard…  [Esc/q] back…"
	}
	width, height := max(1, m.width), max(1, m.height)
	if len(lines) > height-1 {
		lines = lines[:height-1]
	}
	for len(lines) < height-1 {
		lines = append(lines, "")
	}
	lines = append(lines, styles.KeyHints(footer, styles.Dir))
	for i := range lines {
		lines[i] = ansi.Truncate(lines[i], width, "…")
	}
	return strings.Join(lines, "\n")
}
