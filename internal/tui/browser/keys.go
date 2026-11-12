package browser

import (
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/styles"
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
	keyE         = "e"
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

// helpText shows actions for the active pane and mode.
func (m Model) helpText(width int) string {
	if m.filterMode {
		return styles.FitKeyHints(width, keyHint("Enter", "done"), keyHint("Esc", "done"))
	}
	if m.remoteBusy() || m.localReload != nil || len(m.localReads) > 0 {
		return styles.FitKeyHints(width, keyHint("Esc", "cancel"), keyHint("?", "help"))
	}
	hidden, ignored := styles.Muted.Render("hidden"), styles.Muted.Render("ignored")
	if m.showHidden {
		hidden = styles.ActiveHint.Render("hidden")
	}
	if m.showIgnored {
		ignored = styles.ActiveHint.Render("ignored")
	}
	hints := []string{keyHint("s", "compare"), keyHint("Space", "mark")}
	entry := m.remoteCurrent()
	if m.activePane == PaneRemote && m.remoteHost != nil && m.remoteConn != nil && m.remoteConn.Err() == nil && entry != nil && entry.Kind == fs.EntryFile && entry.Mode.IsRegular() && !entry.Unmapped && !entry.Class.HardExcluded {
		select {
		case <-m.remoteConn.Done():
		default:
			hints = append(hints, keyHint("e", "edit remote"))
		}
	}
	hints = append(hints,
		keyHint("Tab", "pane"),
		keyHint("f", "find"),
		styles.Dir.Render("[.]")+hidden,
		styles.Dir.Render("[I]")+ignored,
		keyHint("@", "remote"),
		keyHint("q", "quit"),
		keyHint("?", "help"),
	)
	return styles.FitKeyHints(width, hints...)
}

// PreviewHelpText returns key hints for the active preview.
func PreviewHelpText(width int) string {
	return styles.FitKeyHints(width,
		keyHint("p", "close"),
		keyHint("c", "copy"),
		keyHint("PgUp/PgDown", "scroll"),
		keyHint("drag", "select"),
		keyHint("Home/End", "jump"),
		keyHint("?", "help"),
	)
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

  Find & Compare
  ──────────────────────────────
  f              fuzzy find files across the project, mark with Space
  s              compare marked local/remote files (uses remote pane host when selected)
  @              choose/change host for the remote pane
  e              edit current remote file in the configured external editor
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
