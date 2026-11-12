# Key bindings and mouse

`?` shows the bindings of the current screen inside drift.

## Project dashboard

| Key | Action |
| ----- | -------- |
| `j` / `k` or `↑` / `↓` | Navigate |
| `Enter` | Open project (re-root drift into it) |
| `1`–`9` | Open the n-th project in the filtered list |
| `/` | Filter by name, slug or path; `Enter` finishes input without opening a project |
| `n` | New project |
| `e` | Edit project |
| `d` | Remove project (with confirmation) |
| `a` | Archive / unarchive project |
| `.` | Show / hide archived projects |
| Click / wheel | Move the cursor; double click opens |
| `Esc` | Clear the filter first; then back to the browser when opened from `P` then `m`, otherwise quit |
| `q` | Quit drift |

## Project switcher (`P` in the file browser)

| Key | Action |
| ----- | -------- |
| type | Filter by name, slug, or path |
| `↑` / `↓` or `Ctrl+n` / `Ctrl+p` | Navigate |
| `Enter` | Open the selected project |
| `m` | Open the dashboard to manage projects (empty filter only) |
| `Esc` | Back to the browser (session kept) |

## File browser

| Key | Action |
| ----- | -------- |
| `j` / `k` or `↑` / `↓` | Navigate in the active pane |
| `h` / `←`, `l` / `→` / `Enter` | Collapse / open directory |
| `g` / `G` | Jump to top / bottom |
| `Tab` | Switch local / remote pane |
| `p` | Toggle a text preview in the opposite pane |
| `e` | Edit the current remote file in the configured external editor |
| `c` | Copy the loaded preview content to the clipboard |
| `PgUp` / `PgDn`, `Home` / `End` | Scroll / jump within an active preview |
| `@` | Choose or change the remote host |
| `Space` | Mark / unmark file or directory in the active pane |
| `V` / `*` | Mark the current level / invert the active pane's selection |
| `f` | Fuzzy-find files across the project and mark them |
| `.` / `I` | Toggle hidden / gitignored entries |
| `s` | Sync marked local and remote files |
| `r` / `/` / `?` | Refresh active pane / filter / help |
| `H` | Open host manager |
| `P` | Switch project (filterable picker; `m` opens the dashboard) |
| `Esc` | Clear filter and selections |
| `q` / `Ctrl+C` | Quit (cancels a running connect or sync first) |

## Diff view

| Key | Action |
| ----- | -------- |
| `j` / `k` or `↑` / `↓` | Scroll diff content by line |
| `PgUp` / `PgDn` | Scroll diff content by page |
| `Ctrl+u` / `Ctrl+d` | Scroll diff content by half page |
| `Home` / `g`, `End` / `G` | Jump to start / end of diff content |
| `←` / `→` | Scroll code horizontally without moving the gutters |
| `w` | Switch line wrapping off / on (on by default) |
| `v` | Cycle Unified / Split / Auto layout |
| `?` | Open diff help |
| `[` / `]` | Jump to the previous / next hunk, continuing across files |
| `Enter` / `l` | Expand or collapse the first fold in view |
| `h` | Collapse the fold around the top of the viewport |
| `c` | Expand or collapse every fold in the file |
| `L` | Show 10 more unchanged lines on each changed side of the first fold in view |
| `+` / `-` | More / less context around every change (0, 1, 3, 5, 10, 25 lines) |
| `o` | Show / hide the diff of a file whose changed lines are 4 KiB or longer |
| `Tab` / `Shift+Tab` | Select next / previous file |
| `<` / `>` | Narrow / widen the file list for this session |
| `f` | Show / hide the full local and remote paths |
| `m` | Switch between one file and all files in one scrollable view |
| `z` | In the all-files view, fold the current file to its title |
| `Space` | Cycle sync direction for current file |
| `A` | Cycle sync direction for all files |
| `s` | Sync current file |
| `S` | Sync all files |
| `u` / `d` | Quick upload / download current file |
| `e` | Toggle the last bulk-sync error list (when errors occurred) |
| `i` | Rebuild scope with / without gitignored paths |
| `r` | Reload ignore rules, scope, and diffs |
| `q` / `Esc` | Back to browser |

## Host manager

| Key | Action |
| ----- | -------- |
| `n` | New host. In the project section, choose a new connection or a link to a server |
| `l` | Link a global server, or a host of another project, to the open project |
| `e` / `Enter` | Edit host |
| `c` | Duplicate host into a pre-filled form with a free name |
| `d` | Delete host |
| `t` | Test connection |
| `r` | Reset certificate trust of an FTPS host |
| `q` / `Esc` | Back |

## Mouse

The mouse works in the file browser, the diff view and the host manager.

| Action | Effect |
| -------- | -------- |
| Wheel | Scroll the pane under the pointer |
| Click | Move the cursor there, and focus that pane |
| Click on a pane label | Focus that pane |
| Click on a fold marker | Expand or collapse that fold (diff view) |
| Double click | Expand a directory (local), open one (remote), cycle a file's sync direction (diff view), edit a host (host manager) |
| Drag while previewing | Select preview text with the terminal; mouse reporting resumes when the preview closes |
| `Shift`+Click | Select text with the terminal's own selection |

Mouse reporting pauses while a file preview is open, so you can select preview text
without holding `Shift`. Outside the preview, you can turn it off entirely:

```bash
drift --no-mouse              # this run only
DRIFT_NO_MOUSE=1 drift        # via environment
```

```toml
# ~/.config/drift/config.toml, permanently
[ui]
mouse = false
```

The flag beats the environment variable, which beats the config file. Mouse support
is on unless one of them turns it off.
