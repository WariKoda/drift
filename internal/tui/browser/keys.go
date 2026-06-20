package browser

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
	keySlash     = "/"
	keyEsc       = "esc"
	keyQuestion  = "?"
	keyQ         = "q"
	keyCtrlC     = "ctrl+c"
	keyBackspace = "backspace"
	keyTab       = "tab"
	keyAt        = "@"
)

// HelpText returns the key hint shown in the status bar.
func HelpText() string {
	return "[Tab]pane  [@]remote  [f]find  [s]sync  [H]hosts  [P]projects  [?]help  [q]quit"
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

  Selection
  ──────────────────────────────
  Space          toggle mark in active pane
  v              start visual selection
  V              mark all in current dir of active pane
  *              invert selection in active pane
  Esc            clear filter / selections

  Find & Sync
  ──────────────────────────────
  f              fuzzy find files across the project, mark with Space
  s              sync marked local/remote files (uses remote pane host when selected)
  @              choose/change host for the remote pane
  Tab            switch active pane

  Other
  ──────────────────────────────
  H              host manager
  P              project dashboard
  r              refresh active pane
  /              filter entries
  ?              toggle this help
  q / Ctrl+C     quit`
}
