# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]
### Added
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
- redesigned dashboard with a centered DRIFT banner, project list and bottom action bar
- quick-open projects from the dashboard with number keys `1`–`9`
- browser status bar / help now lists `[H]hosts` and `[P]projects` (return to the dashboard from inside a project)

### Changed
- extract the form text-input widget into `internal/tui/textfield` (shared by host and project forms)

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
