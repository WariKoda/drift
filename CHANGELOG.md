# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]
### Fixed
- a directory marked in the remote pane of an FTP or FTPS host ended up in the diff view as a file, with a red "is a directory" where the diff belongs, instead of being expanded into the files below it. `Stat` treated a successful `SIZE` as proof of a file, but vsftpd, ProFTPD and others answer `SIZE` for directories too. It asks `MLST` first now, which reports the entry type. One command also replaces `SIZE` plus `MDTM`, and its timestamps have second precision, so the diff loader skips more downloads on files that match. Servers without `MLST` keep the old behaviour and the old ambiguity

### Removed
- **breaking:** the migration path for configuration from 0.1.6-alpha and earlier. drift no longer reads `<project>/.drift/config.toml`, `~/.config/drift/secrets.toml` or `~/.config/drift/access.toml`. 0.1.7-alpha is the release that moves them into the project store, so an installation coming from 0.1.6-alpha or earlier has to run 0.1.7-alpha once per project before upgrading; skipping it leaves those hosts in files nothing reads
- `internal/config/gitguard.go` and the notice about a committed project config still holding its password in git history, along with the status-line warning they fed

## [0.1.7-alpha] - 2026-09-03
### Changed
- **breaking:** nothing is stored in the project directory any more. A project's hosts and mappings live in `~/.config/drift/projects/<slug>.toml` (mode `600`), named after its registry slug. No `.drift/`, nothing to add to `.gitignore`, nothing to commit by accident
- **breaking:** which project a directory belongs to is the registry's answer now — the registered project whose path is the directory or a parent of it, longest match first. Registering a project is what gives a directory hosts of its own, and an older drift will not find a migrated project at all, because its lookup walks up for a file that is gone
- mappings no longer travel with a clone. That is the point of the change and the cost of it: every developer enters them once per machine, and moving a directory in the repo no longer has a commit that fixes the mapping for everyone
- config files are replaced atomically (temporary file, then rename), so an interrupted write leaves either the old file or the new one
- saving one project host no longer rewrites the others' records: `[defaults]` values stay in `[defaults]` instead of being baked into every host
- empty fields and zero ports are omitted from written config files
- `~/.config/drift/` is created with mode `700` instead of `755`

### Added
- drift offers to register a repository that no project covers, suggesting the repository root instead of the subdirectory you happened to start in; `n` on the dashboard prefills the same
- configuration from older versions is migrated on startup and on project switch: `.drift/config.toml` in the project, `~/.config/drift/secrets.toml` (0.1.6-alpha) and `~/.config/drift/access.toml`, all folded into the project store, then removed. `.drift/` goes with them when nothing but drift's own `.gitignore` is left in it; a `.drift/` holding anything else is left alone
- the migration reports what moved, and says once that a committed `.drift/config.toml` still has its password in the repository's history, where deleting the file later does not reach it
- an unregistered directory with a leftover `.drift/config.toml` is offered for registration first, since the store is named after the slug; answering `y` registers and migrates in one go
- drift stays in the working directory instead of reopening the last project when that directory is one it can offer to register, so the offer is not hidden

### Fixed
- `go test ./...` wrote into the developer's real `~/.config/drift`, because storing a host reaches the config directory and several tests did not isolate `$XDG_CONFIG_HOME`

## [0.1.6-alpha] — never released

Prepared but never tagged, and superseded by 0.1.7-alpha before it was. It moved
credentials into `~/.config/drift/secrets.toml` and kept a committable project
config; 0.1.7-alpha moves the whole project config out of the repository
instead, which reverses the part of this that users would have seen. Kept here
because the repository went through it, and because 0.1.7-alpha migrates a
`secrets.toml` written by a build from this period.

- project host credentials in `~/.config/drift/secrets.toml` (mode `600`), keyed by project root and host name, instead of `.drift/config.toml`
- credentials still in a `.drift/config.toml` moved into that store on startup, with a status-line report of where they went and whether git could reach the file they came from
- empty `auth` fields no longer written to config files
- the post-save warning about a git-reachable project config from 0.1.5-alpha removed

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
