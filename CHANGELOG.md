# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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
