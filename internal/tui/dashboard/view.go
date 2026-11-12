package dashboard

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/logo"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// accent is used for the logo and active items.
var (
	logoStyle   = styles.Accent
	accentStyle = styles.Accent
)

// logoLines returns the wordmark, or the icon alone when the wordmark does
// not fit. Both are logoRowCount rows tall, so the layout stays the same.
func logoLines(width int) []string {
	wordmark := logo.Lines(logo.Wordmark)
	if lipgloss.Width(wordmark[0]) <= width {
		return wordmark
	}
	return logo.Lines(logo.Icon)
}

func (m Model) View() string {
	w, h := m.termSize()

	center := func(s string) string {
		s = ansi.Truncate(s, w, "…")
		pad := (w - lipgloss.Width(s)) / 2
		if pad < 0 {
			pad = 0
		}
		return strings.Repeat(" ", pad) + s
	}

	var lines []string

	// ── Top padding + logo ────────────────────────────────────────────
	if h < 20 {
		lines = append(lines, center(styles.Header.Render("drift")+"  "+styles.File.Render("Projects")), "")
	} else {
		for i := 0; i < headerBlankTop; i++ {
			lines = append(lines, "")
		}
		for _, l := range logoLines(w) {
			lines = append(lines, center(logoStyle.Render(l)))
		}
		for i := 0; i < headerBlankBot; i++ {
			lines = append(lines, "")
		}
	}
	if m.filterMode || m.filter.Value() != "" {
		field := m.filter
		field.Focused = m.filterMode
		field.Width = max(1, m.blockWidth()-19)
		lines = append(lines, center(field.View()))
	}

	// ── Project rows ──────────────────────────────────────────────────
	footer := m.footerBlock(center)
	for _, row := range m.projectRows() {
		lines = append(lines, center(row))
	}

	// ── Push footer to the bottom ─────────────────────────────────────
	for len(lines)+m.footerHeight() < h {
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
//	listTop()                                       logo or compact header, filter
//	listMax()                                        project rows
//	footerHeight()                                   action bar, status
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

// listTop is the first project row, below the responsive header and filter.
func (m Model) listTop() int {
	_, h := m.termSize()
	top := headerBlankTop + logoRowCount + headerBlankBot
	if h < 20 {
		top = 2
	}
	if m.filterMode || m.filter.Value() != "" {
		top++
	}
	return top
}

func (m Model) footerHeight() int {
	_, h := m.termSize()
	if h < 20 {
		return 2
	}
	return footerLineCount
}

// listMax is how many project rows fit between the logo and the footer.
func (m Model) listMax() int {
	_, h := m.termSize()
	return max(0, h-m.listTop()-m.footerHeight())
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
	maxFit := max(1, w-2)
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
	if m.listMax() == 0 {
		return nil
	}
	if len(m.entries) == 0 {
		if m.filter.Value() != "" {
			return []string{styles.Muted.Render("No matching projects")}
		}
		return []string{styles.Muted.Render("No projects yet — press ") +
			styles.Dir.Render("n") + styles.Muted.Render(" to add one.")}
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
	name := st.Render(padRune(styles.SafeText(e.proj.Name), m.nameWidth()))
	path := styles.Muted.Render(padRuneLeft(styles.SafeText(collapseHome(e.proj.Path)), pathWidth))

	// Right-aligned quick-open key for the first nine projects.
	key := " "
	if i < 9 {
		key = fmt.Sprintf("%d", i+1)
	}
	return ansi.Truncate(diamond+name+" "+path+" "+styles.Dir.Render(key), m.blockWidth(), "…")
}

// footerBlock renders the action hints and the status line, bottom-pinned.
func (m Model) footerBlock(center func(string) string) []string {
	var status string
	switch {
	case m.confirmDelete:
		if e := m.currentEntry(); e != nil {
			status = styles.Warn.Render(fmt.Sprintf("Remove %q and its saved hosts?", styles.SafeText(e.proj.Name)))
		}
	case m.statusMsg != "":
		status = styles.Err.Render(m.statusMsg)
	default:
		unit := "projects"
		if len(m.entries) == 1 {
			unit = "project"
		}
		status = styles.Muted.Render(fmt.Sprintf("%d %s", len(m.entries), unit))
		if m.filter.Value() != "" {
			status = styles.Muted.Render(fmt.Sprintf("%d of %d projects", len(m.entries), len(m.allEntries)))
		}
		if m.showArchived {
			status += styles.Muted.Render(" · including archived")
		}
	}
	if m.footerHeight() == 2 {
		return []string{center(m.actionBar()), center(status)}
	}

	return []string{
		center(m.actionBar()),
		"",
		center(status),
		"",
	}
}

func (m Model) actionBar() string {
	w, _ := m.termSize()
	if m.confirmDelete {
		return styles.FitKeyHints(w, styles.KeyHints("[y]remove", styles.Warn), styles.KeyHints("[n]cancel", styles.Muted))
	}
	if m.filterMode {
		return styles.FitKeyHints(w, styles.KeyHints("[↑↓]move", styles.Muted), styles.KeyHints("[Enter]done", styles.Muted), styles.KeyHints("[Esc]clear filter", styles.Muted))
	}
	pairs := [][2]string{
		{"↵", "open"}, {"/", "filter"},
	}
	if m.filter.Value() != "" || m.returnable {
		pairs = append(pairs, [2]string{"q", "quit"})
	}
	pairs = append(pairs, [2]string{"n", "new"}, [2]string{"e", "edit"}, [2]string{"d", "remove"}, [2]string{"a", "archive"}, [2]string{".", "archived"})
	switch {
	case m.filter.Value() != "":
		pairs = append(pairs, [2]string{"Esc", "clear filter"})
	case m.returnable:
		pairs = append(pairs, [2]string{"Esc", "back"})
	default:
		pairs = append(pairs, [2]string{"q", "quit"})
	}
	var parts []string
	for _, p := range pairs {
		parts = append(parts, styles.Dir.Render("["+p[0]+"]")+styles.Muted.Render(" "+p[1]))
	}
	return styles.FitKeyHints(w, parts...)
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

// padRune truncates s to terminal columns and pads it to width.
func padRune(s string, width int) string {
	width = max(0, width)
	s = ansi.Truncate(s, width, "…")
	return s + strings.Repeat(" ", width-ansi.StringWidth(s))
}

// padRuneLeft keeps the tail of s (most relevant for paths) within width.
func padRuneLeft(s string, width int) string {
	if width <= 0 {
		return ""
	}
	if cells := ansi.StringWidth(s); cells > width {
		s = ansi.TruncateLeft(s, cells-width+1, "…")
	}
	return s + strings.Repeat(" ", width-ansi.StringWidth(s))
}
