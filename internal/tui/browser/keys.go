package browser

import (
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// key constants — used in Update's switch statements.
const (
	keyDown      = "down"
	keyUp        = "up"
	keyRight     = "right"
	keyLeft      = "left"
	keyJ         = "j"
	keyK         = "k"
	keyL         = "l"
	keyH         = "h"
	keyEnter     = "enter"
	keySpace     = " "
	keyG         = "g"
	keyShiftG    = "G"
	keyV         = "v"
	keyShiftV    = "V"
	keyStar      = "*"
	keyS         = "s"
	keyR         = "r"
	keyP         = "p"
	keyC         = "c"
	keyPgUp      = "pgup"
	keyPgDown    = "pgdown"
	keyHome      = "home"
	keyEnd       = "end"
	keySlash     = "/"
	keyEsc       = "esc"
	keyQuestion  = "?"
	keyQ         = "q"
	keyCtrlC     = "ctrl+c"
	keyBackspace = "backspace"
	keyTab       = "tab"
	keyAt        = "@"
	keyDot       = "."
	keyShiftI    = "I"
)

// HelpText returns the key hints and current visibility states shown in the status bar.
func HelpText(showHidden, showIgnored bool, width int) string {
	hidden, ignored := styles.Muted.Render("hidden"), styles.Muted.Render("ignored")
	if showHidden {
		hidden = styles.ActiveHint.Render("hidden")
	}
	if showIgnored {
		ignored = styles.ActiveHint.Render("ignored")
	}
	return fitKeyHints(width,
		keyHint("s", "sync"),
		styles.Dir.Render("[.]")+hidden,
		styles.Dir.Render("[I]")+ignored,
		keyHint("Tab", "pane"),
		keyHint("q", "quit"),
		keyHint("@", "remote"),
		keyHint("f", "find"),
		keyHint("?", "help"),
	)
}

// PreviewHelpText returns key hints for the active preview.
func PreviewHelpText(width int) string {
	return fitKeyHints(width,
		keyHint("p", "close"),
		keyHint("c", "copy"),
		keyHint("PgUp/PgDown", "scroll"),
		keyHint("drag", "select"),
		keyHint("Home/End", "jump"),
		keyHint("?", "help"),
	)
}

// The last hint is the escape/help action and always gets space first.
func fitKeyHints(width int, hints ...string) string {
	last := hints[len(hints)-1]
	if lipgloss.Width(last) > width {
		return ""
	}
	remaining := width - lipgloss.Width(last)
	var selected []string
	for _, hint := range hints[:len(hints)-1] {
		needed := lipgloss.Width(hint) + 2
		if needed <= remaining {
			selected = append(selected, hint)
			remaining -= needed
		}
	}
	return strings.Join(append(selected, last), "  ")
}

func keyHint(key, label string) string {
	return styles.Dir.Render("["+key+"]") + styles.Muted.Render(label)
}

// FullHelp returns the help overlay text.
func FullHelp() string {
	return `  Navigation
  ──────────────────────────────
  j / ↓          cursor down
  k / ↑          cursor up
  l / → / Enter  expand dir
  h / ←          collapse dir / go to parent
  g              jump to top
  G              jump to bottom

  Preview
  ──────────────────────────────
  p              toggle file preview in opposite pane
  c              copy loaded preview to clipboard
  Mouse drag     select preview text with the terminal
  PgUp / PgDown  scroll preview by page
  Home / End     jump to preview start / end

  Selection
  ──────────────────────────────
  Space          toggle mark in active pane
  v              start / finish visible range selection
  V              mark all in current dir of active pane
  *              invert selection in active pane
  Esc            clear filter / selections

  Find & Sync
  ──────────────────────────────
  f              fuzzy find files across the project, mark with Space
  s              sync marked local/remote files (uses remote pane host when selected)
  @              choose/change host for the remote pane
  .              toggle hidden files
  I              toggle gitignored files
                 highlighted/underlined labels indicate enabled visibility
  Tab            switch active pane

  Mouse
  ──────────────────────────────
  Wheel          scroll the pane under the pointer
  Click          move the cursor there / focus that pane
  Double click   expand dir (local) or open dir (remote)
  Shift+Click    let the terminal select text again
  --no-mouse     start without mouse reporting

  Other
  ──────────────────────────────
  H              host manager
	P              switch project (Esc back, m to manage)
  r              refresh active pane
  /              filter entries
  ?              toggle this help
  q / Ctrl+C     quit`
}
