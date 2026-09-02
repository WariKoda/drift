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
	w, h := m.termSize()

	center := func(s string) string {
		pad := (w - lipgloss.Width(s)) / 2
		if pad < 0 {
			pad = 0
		}
		return strings.Repeat(" ", pad) + s
	}

	var lines []string

	// ── Top padding + logo ────────────────────────────────────────────
	for i := 0; i < headerBlankTop; i++ {
		lines = append(lines, "")
	}
	for _, l := range logoLines() {
		lines = append(lines, center(logoStyle.Render(l)))
	}
	for i := 0; i < headerBlankBot; i++ {
		lines = append(lines, "")
	}

	// ── Project rows ──────────────────────────────────────────────────
	footer := m.footerBlock(center)
	for _, row := range m.projectRows() {
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

// Screen layout, top to bottom:
//
//	headerBlankTop + logoRowCount + headerBlankBot   padding and logo
//	listMax()                                        project rows
//	footerLineCount                                  action bar, status
//
// hitTest in mouse.go maps clicks back through these helpers, so a layout
// change here must be a change to the constants — not a hand-counted number
// in View.
const (
	headerBlankTop  = 1
	logoRowCount    = 5
	headerBlankBot  = 2
	footerLineCount = 4

	defaultTermWidth  = 80
	defaultBlockWidth = 58
	minBlockWidth     = 40
	maxBlockWidth     = 88
	defaultNameWidth  = 18
	maxNameWidth      = 36
)

func (m Model) termSize() (w, h int) {
	w, h = m.Width, m.Height
	if w < 1 {
		w = defaultTermWidth
	}
	if h < 1 {
		h = 24
	}
	return
}

// listTop is the y of the first project row (1 blank + 5 logo + 2 blanks).
func (m Model) listTop() int {
	return headerBlankTop + logoRowCount + headerBlankBot
}

// listMax is how many project rows fit between the logo and the footer.
func (m Model) listMax() int {
	_, h := m.termSize()
	n := h - m.listTop() - footerLineCount
	if n < 1 {
		return 1
	}
	return n
}

// windowStart is the first entry index shown, keeping the cursor in view
// when the list is longer than listMax.
func (m Model) windowStart() int {
	max := m.listMax()
	if len(m.entries) <= max {
		return 0
	}
	start := m.cursor - max/2
	if start < 0 {
		start = 0
	}
	if start > len(m.entries)-max {
		start = len(m.entries) - max
	}
	return start
}

// blockWidth is the visual width of the centered project block.
//
// An 80-col terminal keeps the original 58-cell block. Wider terminals grow
// it toward maxBlockWidth; narrower ones shrink it so it still fits, never
// below minBlockWidth.
func (m Model) blockWidth() int {
	w, _ := m.termSize()
	bw := w - (defaultTermWidth - defaultBlockWidth) // keep the 80-col side pad
	if bw > maxBlockWidth {
		bw = maxBlockWidth
	}
	if bw < minBlockWidth {
		bw = minBlockWidth
	}
	maxFit := w - 2
	if maxFit < minBlockWidth {
		maxFit = minBlockWidth
	}
	if bw > maxFit {
		bw = maxFit
	}
	return bw
}

// nameWidth is how many runes the name column gets inside the block.
func (m Model) nameWidth() int {
	nw := defaultNameWidth + (m.blockWidth() - defaultBlockWidth)
	if nw < defaultNameWidth {
		return defaultNameWidth
	}
	if nw > maxNameWidth {
		return maxNameWidth
	}
	return nw
}

// blockLeft is the x of the centered block — the same pad View's center()
// applies to a row whose visual width is blockWidth.
func (m Model) blockLeft() int {
	w, _ := m.termSize()
	pad := (w - m.blockWidth()) / 2
	if pad < 0 {
		pad = 0
	}
	return pad
}

// projectRows renders up to listMax project rows (a window around the cursor).
func (m Model) projectRows() []string {
	if len(m.entries) == 0 {
		return []string{styles.Muted.Render("No projects yet — press ") +
			accentStyle.Render("n") + styles.Muted.Render(" to add one.")}
	}

	start := m.windowStart()
	end := start + m.listMax()
	if end > len(m.entries) {
		end = len(m.entries)
	}

	pathWidth := m.blockWidth() - 2 - m.nameWidth() - 1 - 2 // diamond + name + sep + key area
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
	name := st.Render(padRune(e.proj.Name, m.nameWidth()))
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
		center(m.actionBar()),
		"",
		center(status),
		"",
	}
}

func (m Model) actionBar() string {
	pairs := [][2]string{
		{"↵", "open"}, {"n", "new"}, {"e", "edit"},
		{"d", "remove"}, {"a", "archive"}, {".", "archived"},
	}
	if m.returnable {
		pairs = append(pairs, [2]string{"Esc", "back"})
	}
	pairs = append(pairs, [2]string{"q", "quit"})
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
