# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]
### Added
- `~/.config/drift/access.toml` holds your access to a project host — `user`, `auth` and `insecure_tls` — keyed by project root and host name; the project config describes the environment only (hostname, port, root path, protocol, mappings) and is meant to be committed
- an access field written by hand into `.drift/config.toml` still wins over the store, so a value a team maintains deliberately (a deploy account, a `$DEPLOY_PASSWORD` convention) keeps working
- the host form groups a project host's fields under `SHARED WITH THE TEAM` and `ONLY ON THIS MACHINE`, each naming the file it writes to; a global host has one file and gets no headings
- the host manager's `PROJECT HOSTS` header notes that access stays on this machine, where the terminal is wide enough for it

### Changed
- host form field order follows the two layers: name, the shared environment fields, then the fields that stay on this machine, then the scope toggle
- `insecure_tls` is no longer written into the project config: a skip-verify flag one developer needs is not something the team should inherit by pulling
- a 0.1.6-alpha `secrets.toml` is folded into `access.toml` on startup and removed; migration continues to move literal passwords and passphrases out of the project config, and now deliberately leaves `user`, `auth.type`, `key_file` and `$ENV` references in place — they are not leaks, and deleting them from a shared file would lose values drift never stored for anyone else
- config files are replaced atomically (temporary file, then rename), so an interrupted write can no longer truncate `access.toml` and lose every project's credentials
- saving or deleting a project host starts from the config file on disk instead of the merged in-memory view, so the other hosts' records stay byte-for-byte as they were: `[defaults]` values are not baked into them, and nothing from the access store leaks back into the project config
- empty fields and zero ports are omitted from written config files
- `~/.config/drift/` is created with mode `700` instead of `755`

## [0.1.6-alpha] - 2026-09-03
### Added
- project host credentials live in `~/.config/drift/secrets.toml` (mode `600`), keyed by project root and host name, instead of `.drift/config.toml` — the project config now holds only hostname, port, user, root path, protocol and mappings, and can be committed and shared
- credentials that an older or hand-written `.drift/config.toml` still carries are moved into the secret store on startup and on project switch, and the status line reports where they went and whether git can still reach the file they came from

### Changed
- empty `auth` fields are no longer written to config files

### Removed
- the post-save warning about a git-reachable project config introduced in 0.1.5-alpha: the file drift writes no longer contains a credential, so the migration notice is the only place the git check is still needed

## [0.1.5-alpha] - 2026-09-03
### Added
- writing a project host creates `.drift/.gitignore` (ignoring `config.toml`) so credentials cannot be committed by accident; an existing `.gitignore` is left alone
- drift warns in the status line when `.drift/config.toml` stores a literal password or passphrase and git can still reach it — checked on startup and after each save, dismissed with `Esc`
- unified diff folds unchanged stretches behind `@@` hunk headers, keeping three lines of context: `Enter`/`l` toggles the first fold in view, `h` collapses the one at the top of the viewport, `c` toggles every fold in the file, and a click on a fold marker toggles it

### Changed
- `.drift/` is created with mode `700` instead of `755`
- `[` / `]` walk the hunk headers of the folded view instead of re-deriving hunk starts from the line list

### Fixed
- project switcher treats digit keys as search input instead of jumping to the n-th row

## [0.1.4-alpha] - 2026-09-02
### Added
- project switcher (`P` in the browser): filter by name/slug/path, Esc returns to the current session, `m` opens the dashboard to manage entries
- last-opened project is recorded and restored when `drift` starts outside a project; `--dashboard` still forces the list
- `drift open` accepts a unique name or slug, not only an exact slug
- mouse support on the project dashboard (click to select, double click to open, wheel moves the cursor)
- mouse support in the file browser, diff view and host manager: the wheel scrolls the pane under the pointer, a click moves the cursor and focuses that pane, and a double click performs the row's action (expand a local directory, open a remote one, cycle a file's sync direction, edit a host)
- clicking a browser pane label focuses that pane; the fuzzy finder supports wheel and click, with a double click marking a result
- mouse reporting can be turned off with `--no-mouse`, `DRIFT_NO_MOUSE=1`, or `[ui] mouse = false` in the global config — it otherwise takes the terminal's own text selection away (`Shift`+Click restores it per selection)

### Changed
- dashboard Esc returns to the browser when the dashboard was opened from the project switcher; `q` still quits
- dashboard project names grow with the terminal instead of clipping at 18 characters
- browser header shows the registered project name next to the path
- project list sorts by last opened, then name
- diff viewer shows a unified single-pane diff instead of side-by-side local/remote columns
- diff viewer places the file list on the left and the unified diff on the right

### Fixed
- host manager rendered two lines more than the terminal had, because each section header emitted a blank line the row budget never counted; the status bar was pushed off screen
- stale diff results from an abandoned load (switched project or host) no longer overwrite the current view
- FTP directory walker could deadlock when listings ran in parallel
- local reads, writes and deletes stay inside the project root, including when a path walks through a symlink

## [0.1.3-alpha] - 2026-07-30
### Added
- project-wide fuzzy file finder in the browser (`f`): search the whole project and multi-select files to mark for sync (powered by `sahilm/fuzzy`); results show the filename first with the directory dimmed alongside so look-alike names stay distinguishable
- project dashboard: optional TUI landing screen listing registered projects
- pick a project to re-root drift into it (loads its config, opens the browser there)
- project registry stored in `~/.config/drift/projects.toml` (`internal/project`)
- dashboard actions: open, new, edit, remove, archive/unarchive, toggle archived
- CLI: `drift dash`, `drift open <slug>`, `drift projects list|add|edit|remove|archive`
- start flags `--dashboard` / `--no-dashboard`; dashboard auto-shows outside a project when projects exist
- press `P` in the browser to return to the dashboard
- when started inside an unregistered `.drift` project, drift offers to register it (`[y]` / `[n]`)
- per-host `insecure_tls` option for ftps hosts with self-signed/mismatched certificates (host form toggle + config field)
- diff viewer shows the active file's full local source and remote target path in the column labels
- diff viewer marks removed (`-`) and added (`+`) lines with git-style gutter signs for clearer reading
- diff viewer pairs changed lines side-by-side on one row (old left / new right) instead of separate rows
- diff colouring is now sync-direction-aware: on upload local changes show as additions, on download remote changes do
- redesigned dashboard with a centered DRIFT banner, project list and bottom action bar
- quick-open projects from the dashboard with number keys `1`–`9`
- browser status bar / help now lists `[H]hosts` and `[P]projects` (return to the dashboard from inside a project)
- diff viewer supports line, half-page, full-page, and start/end scrolling within the selected file

### Changed
- extract the form text-input widget into `internal/tui/textfield` (shared by host and project forms)
- keep synchronized diff scrolling within the visible viewport after refreshes, file changes, hunk jumps, and terminal resizes; add `Home` / `End` navigation

## [0.1.2-alpha] - 2026-04-19
### Changed
- make path mapping segment-safe to avoid false matches on similar prefixes
- distinguish real missing files from other stat/protocol errors during diffing
- keep non-not-found errors visible instead of treating them as missing files
- move sync decision logic from `diffview` into `internal/sync`
- process hosts and marked paths deterministically for more stable behavior

## [0.1.1-alpha] - 2026-04-18
### Changed
- improve drift version output for builds installed via `go install`
- keep showing injected versions for release builds
- fall back to Go build metadata for tagged installs
