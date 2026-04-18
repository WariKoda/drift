# AGENTS.md — drift-tui

Guidelines for AI agents working on this codebase.

## Project overview

drift is a standalone terminal TUI (Go + Bubble Tea) for diffing and syncing local files with remote hosts over SFTP or FTP. It is inspired by PHPStorm's "Browse Remote Host" / "Sync with Deployed To" workflow.

## Architecture

The app is a single Bubble Tea root model (`internal/tui/app.go`) that routes messages to one active screen at a time. Screens are Go packages under `internal/tui/`:

| Package | Screen |
|---------|--------|
| `browser` | File browser (entry point) |
| `hostselector` | Modal: pick sync target |
| `hostmanager` | CRUD list of hosts |
| `hostform` | Create / edit a host (includes mapping sub-screen) |
| `diffview` | Side-by-side diff + sync |

Screen transitions happen via typed messages (e.g. `browser.MsgSyncRequested`, `hostselector.MsgHostChosen`). The root model (`app.go`) owns all screen models and handles cross-screen messages.

Remote I/O goes through the `remote.Client` interface (`internal/remote/client.go`). Two implementations exist: `internal/sftp` (SFTP/SSH) and `internal/ftp` (FTP). Use `remote.Connect(ctx, host)` — never instantiate protocol clients directly.

Path translation between local and remote is handled by `internal/pathmap`. When a host has `Mappings` configured, only files within those mappings may be synced. Falls back to `host.RootPath`-relative paths when no mappings are set.

## Code conventions

- **No mocks in tests** — use real connections or skip.
- **No speculative abstractions** — add helpers only when used in 3+ places.
- **No backwards-compat shims** — if something is unused, delete it.
- **No error swallowing** — propagate errors to the TUI as typed messages (`MsgDiffError`, session `.Err` field, etc.).
- All async work runs in `tea.Cmd` goroutines. Never block `Update()`.
- Styles live in `internal/styles/styles.go` and `internal/tui/styles.go`. Do not inline lipgloss styles in view code.
- Config is TOML. Types live in `internal/config/config.go`. Persistence via `internal/config/writer.go`.

## Key types

```go
config.Host         // a remote target (name, hostname, port, auth, root_path, protocol, mappings)
config.Mapping      // {Local, Remote} path pair — relative local, absolute remote
remote.Client       // Stat, ReadFile, WriteFile, UploadFile, DownloadFile, WalkFiles, DeleteFile, Close
diff.Session        // {LocalPath, RemotePath, Result *DiffResult, Err, Loaded}
diff.DiffResult     // comparison output; HasDiff() reports whether files differ
diffview.SyncDir    // DirNone / DirUpload / DirDownload / DirDeleteLocal / DirDeleteRemote
```

## Adding a new screen

1. Create `internal/tui/<name>/model.go`, `view.go`, `update.go`.
2. Define typed entry/exit messages in the package.
3. Add the model as a field on `tui.App`.
4. Add a `Screen<Name>` constant to `internal/tui/state.go`.
5. Handle entry/exit messages and delegate `Update`/`View` in `app.go`.

## Adding a new protocol

1. Implement `remote.Client` in a new package under `internal/`.
2. Add a protocol constant to `config.Host.Protocol`.
3. Add a case to `remote.Connect()`.
4. Add a toggle option to `hostform` (`fProtocol` in `model.go`, `visibleRows`, `view.go`).

## File walker exclusions

`internal/fs/local.go` `WalkFiles` skips `.git`, `.svn`, `.hg`, `node_modules`, `.idea`, `.vscode`. Add entries to `skipDirs` for new exclusions — do not add flags or callbacks.

## Config locations

| Scope | Path |
|-------|------|
| Global | `~/.config/drift/config.toml` (or `$XDG_CONFIG_HOME/drift/config.toml`) |
| Project | `.drift/config.toml` in project root (walked up from cwd) |

Project hosts override global hosts by name. Project `Mappings` are a fallback; host-level `Mappings` take precedence.

## Build & install

```bash
go build -o drift .          # local binary
make install                  # installs to ~/.local/bin/drift
make update                   # rebuild + reinstall (use after code changes)
```

## Git workflow

- Keep `main` stable and releasable.
- Do not work directly on `main` unless the user explicitly asks for it.
- Create a short-lived branch for each change:
  - `feature/...` for new functionality
  - `fix/...` for bug fixes
  - `docs/...` for documentation changes
  - `chore/...` for maintenance
  - `refactor/...` for internal cleanup
- Typical flow:

```bash
git switch main
git pull
git switch -c feature/my-change
# implement change
go test ./...
go vet ./...
go build ./...
git add .
git commit -m "Add my change"
git push -u origin feature/my-change
```

- Open a pull request from the branch into `main`.
- Merge to `main` only after validation passes.
- Create releases from `main` using tags.

Module path: `github.com/WariKoda/drift`
