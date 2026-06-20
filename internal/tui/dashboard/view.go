package dashboard

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// accent is used for the logo and active items.
var (
	logoStyle   = styles.Accent
	accentStyle = styles.Accent
)

// glyphs holds 5-row block-letter art for the title.
var glyphs = map[rune][5]string{
	'D': {"██████ ", "██   ██", "██   ██", "██   ██", "██████ "},
	'R': {"██████ ", "██   ██", "██████ ", "██  ██ ", "██   ██"},
	'I': {"██", "██", "██", "██", "██"},
	'F': {"██████", "██    ", "█████ ", "██    ", "██    "},
	'T': {"███████", "  ██   ", "  ██   ", "  ██   ", "  ██   "},
}

func logoLines() []string {
	const word = "DRIFT"
	rows := make([]string, 5)
	for i := 0; i < 5; i++ {
		var parts []string
		for _, c := range word {
			parts = append(parts, glyphs[c][i])
		}
		rows[i] = strings.Join(parts, " ")
	}
	return rows
}

func (m Model) View() string {
	w, h := m.Width, m.Height
	if w < 1 {
		w = 80
	}
	if h < 1 {
		h = 24
	}

	center := func(s string) string {
		pad := (w - lipgloss.Width(s)) / 2
		if pad < 0 {
			pad = 0
		}
		return strings.Repeat(" ", pad) + s
	}

	var lines []string

	// ── Top padding + logo ────────────────────────────────────────────
	lines = append(lines, "")
	for _, l := range logoLines() {
		lines = append(lines, center(logoStyle.Render(l)))
	}
	lines = append(lines, "", "")

	// ── Project rows ──────────────────────────────────────────────────
	footer := m.footerBlock(center)
	listMax := h - len(lines) - len(footer)
	if listMax < 1 {
		listMax = 1
	}
	for _, row := range m.projectRows(listMax) {
		lines = append(lines, center(row))
	}

	// ── Push footer to the bottom ─────────────────────────────────────
	for len(lines)+len(footer) < h {
		lines = append(lines, "")
	}
	lines = append(lines, footer...)
	if len(lines) > h {
		lines = lines[:h]
	}

	return strings.Join(lines, "\n")
}

// blockWidth is the fixed visual width of the centered menu block.
const (
	blockWidth = 58
	nameWidth  = 18
)

// projectRows renders up to listMax project rows (a window around the cursor).
func (m Model) projectRows(listMax int) []string {
	if len(m.entries) == 0 {
		return []string{styles.Muted.Render("No projects yet — press ") +
			accentStyle.Render("n") + styles.Muted.Render(" to add one.")}
	}

	start := 0
	if len(m.entries) > listMax {
		// keep the cursor in view
		start = m.cursor - listMax/2
		if start < 0 {
			start = 0
		}
		if start > len(m.entries)-listMax {
			start = len(m.entries) - listMax
		}
	}
	end := start + listMax
	if end > len(m.entries) {
		end = len(m.entries)
	}

	pathWidth := blockWidth - 2 - nameWidth - 1 - 2 // diamond + name + sep + key area
	if pathWidth < 8 {
		pathWidth = 8
	}

	var rows []string
	for i := start; i < end; i++ {
		rows = append(rows, m.renderRow(i, pathWidth))
	}
	return rows
}

func (m Model) renderRow(i, pathWidth int) string {
	e := m.entries[i]
	active := i == m.cursor

	// Filled diamond marks the cursor row; colour conveys the project's state.
	marker := "◇ "
	if active {
		marker = "◆ "
	}
	var st lipgloss.Style
	switch {
	case e.missing:
		st = styles.Err
	case e.proj.Archived:
		st = styles.Muted
	case active:
		st = accentStyle
	default:
		st = styles.File
	}
	diamond := st.Render(marker)
	name := st.Render(padRune(e.proj.Name, nameWidth))
	path := styles.Muted.Render(padRuneLeft(collapseHome(e.proj.Path), pathWidth))

	// Right-aligned quick-open key for the first nine projects.
	key := " "
	if i < 9 {
		key = fmt.Sprintf("%d", i+1)
	}
	keyStyle := styles.Muted
	if active {
		keyStyle = accentStyle
	}

	return diamond + name + " " + path + " " + keyStyle.Render(key)
}

// footerBlock renders the action hints and the status line, bottom-pinned.
func (m Model) footerBlock(center func(string) string) []string {
	var status string
	switch {
	case m.confirmDelete:
		if e := m.currentEntry(); e != nil {
			status = styles.Warn.Render(fmt.Sprintf("Remove %q from the registry?  ", e.proj.Name)) +
				accentStyle.Render("y") + styles.Muted.Render("es  ") +
				accentStyle.Render("n") + styles.Muted.Render("o")
		}
	case m.statusMsg != "":
		status = styles.Err.Render(m.statusMsg)
	default:
		archived := "show archived"
		if m.showArchived {
			archived = "hide archived"
		}
		status = styles.Muted.Render(fmt.Sprintf("%d project(s) · %s", len(m.entries), archived))
	}

	return []string{
		center(actionBar()),
		"",
		center(status),
		"",
	}
}

func actionBar() string {
	pairs := [][2]string{
		{"↵", "open"}, {"n", "new"}, {"e", "edit"},
		{"d", "remove"}, {"a", "archive"}, {".", "archived"}, {"q", "quit"},
	}
	var parts []string
	for _, p := range pairs {
		parts = append(parts, styles.Key.Render("["+p[0]+"]")+styles.Muted.Render(" "+p[1]))
	}
	return strings.Join(parts, styles.Muted.Render("   "))
}

// collapseHome rewrites a path under $HOME to use ~ for display.
func collapseHome(p string) string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return p
	}
	if p == home {
		return "~"
	}
	if rel, err := filepath.Rel(home, p); err == nil && !strings.HasPrefix(rel, "..") {
		return "~" + string(filepath.Separator) + rel
	}
	return p
}

// padRune truncates s to max runes (with an ellipsis) and pads it to width.
func padRune(s string, width int) string {
	r := []rune(s)
	if len(r) > width {
		if width <= 1 {
			return string(r[:width])
		}
		return string(r[:width-1]) + "…"
	}
	return s + strings.Repeat(" ", width-len(r))
}

// padRuneLeft keeps the tail of s (most relevant for paths) within width.
func padRuneLeft(s string, width int) string {
	r := []rune(s)
	if len(r) > width {
		if width <= 1 {
			return string(r[len(r)-width:])
		}
		return "…" + string(r[len(r)-(width-1):])
	}
	return s + strings.Repeat(" ", width-len(r))
}
