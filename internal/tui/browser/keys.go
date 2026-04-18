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
)

// HelpText returns the key hint shown in the status bar.
func HelpText() string {
	return "[s]sync  [Space]mark  [r]refresh  [?]help  [q]quit"
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
  Space          toggle mark
  v              start visual selection
  V              mark all in current dir
  *              invert selection
  Esc            clear filter / selection

  Sync
  ──────────────────────────────
  s              open host selector (requires ≥1 mark)

  Other
  ──────────────────────────────
  r              refresh directory
  /              filter entries
  ?              toggle this help
  q / Ctrl+C     quit`
}
