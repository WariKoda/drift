# AGENTS.md — drift-tui

Guidelines for AI agents working on this codebase.

## Project overview

drift is a standalone terminal TUI (Go + Bubble Tea) for browsing, diffing, and syncing local files with remote hosts over SFTP, FTP, or FTPS. It is inspired by PHPStorm's "Browse Remote Host" / "Sync with Deployed To" workflow.

## Architecture

The app is a single Bubble Tea root model (`internal/tui/app.go`) that routes messages to one active screen at a time. Screens are Go packages under `internal/tui/`:


| Package             | Screen                                                |
| ------------------- | ----------------------------------------------------- |
| `dashboard`         | Project dashboard (optional landing screen)           |
| `projectform`       | Create / edit a project                               |
| `projectselector`   | Modal: switch project from the browser (`P`)          |
| `browser`           | File browser (entry point)                            |
| `hostselector`      | Modal: pick sync target                               |
| `hostmanager`       | CRUD list of hosts                                    |
| `hostform`          | Create / edit a host (includes mapping sub-screen)    |
| `diffview`          | File list + unified diff + sync                       |


The `textfield` package holds the shared single-line input widget used by `hostform`
and `projectform`. The project registry (slug, name, path, timestamps) lives in
`internal/project` and is persisted to `~/.config/drift/projects.toml`; per-project hosts
stay in `<path>/.drift/config.toml`. Selecting a project on the dashboard or in the
project switcher (`P`) re-roots the app via `App.openProject` (`config.Load(path)` +
`browser.New(path)`). `P` opens the switcher as a modal and leaves the browser session
intact until a different project is chosen.

Screen transitions happen via typed messages (e.g. `browser.MsgSyncRequested`, `hostselector.MsgHostChosen`). The root model (`app.go`) owns all screen models and handles cross-screen messages.

Remote I/O goes through the `remote.Client` interface and connection factory (`internal/remote/client.go`). Two implementations exist: `internal/sftp` (SFTP/SSH) and `internal/ftp` (FTP or explicit-TLS FTPS). Use `remote.Connect(ctx, host)` — never instantiate protocol clients directly.

Path translation between local and remote is handled by `internal/pathmap`. When effective host-level or project-level fallback mappings are configured, only files within those mappings may be synced. Paths otherwise fall back to `host.RootPath`-relative translation.

## Code conventions

- **No mocks in tests** — use real connections or skip.
- **No speculative abstractions** — add helpers only when used in 3+ places.
- **No backwards-compat shims** — if something is unused, delete it.
- **No error swallowing** — propagate errors to the TUI as typed messages (`MsgDiffError`, session `.Err` field, etc.). Also log connect/sync/diff failures via `internal/log` (see Logging) so they survive past the TUI session.
- All async work runs in `tea.Cmd` goroutines. Never block `Update()`.
- Styles live in `internal/styles/styles.go` and `internal/tui/styles.go`. Do not inline lipgloss styles in view code.
- Config is TOML. Types live in `internal/config/config.go`. Persistence via `internal/config/writer.go`.



## Key types

```go
config.Host         // a remote target (name, hostname, port, auth, root_path, protocol, mappings)
config.Mapping      // {Local, Remote} path pair — local relative to project root, remote relative to Host.RootPath
remote.Client       // Stat, ReadDir, Open, ReadFile, WriteFile, UploadFile, DownloadFile, WalkFiles, DeleteFile, Close
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
2. Add the protocol value and connection case to `remote.Connect()`.
3. Add the corresponding `hostform.Protocol` value and toggle option (`model.go`, `update.go`, `view.go`).
4. Update `hostform.visibleRows()` when the protocol needs protocol-specific fields.



## File walker exclusions

`internal/fs/local.go` `WalkFiles` skips `.git`, `.svn`, `.hg`, `node_modules`, `.idea`, `.vscode`. Add entries to `skipDirs` for new exclusions — do not add flags or callbacks.

## Config locations


| Scope    | Path                                                                    |
| -------- | ----------------------------------------------------------------------- |
| Global   | `~/.config/drift/config.toml` (or `$XDG_CONFIG_HOME/drift/config.toml`) |
| Project  | `.drift/config.toml` in project root (walked up from cwd)               |
| Registry | `~/.config/drift/projects.toml` (project list; via `config.Dir()`)      |
| Access   | `~/.config/drift/access.toml` (per-user access to project hosts)        |


Project hosts override global hosts by name. Project `Mappings` are a fallback; host-level `Mappings` take precedence.

## Config layers

A project host is stored in two layers with different owners, and
`internal/config/access.go` is the seam:

| `.drift/config.toml` (project, committable) | `<config.Dir()>/access.toml` (per user) |
| --- | --- |
| `hostname`, `port`, `root_path`, `protocol`, `mappings` | `user`, `auth`, `insecure_tls` |

- `splitAccess` is the **write** path: it cuts every access field out, including
  `$ENV` references, because the mechanism is per person even when the value is
  not a secret. `SaveProjectHost` stores that half and writes the rest.
- `splitSecret` is the **migration** path: it moves literal passwords and
  passphrases only. `MigrateProjectSecrets(projectRoot)` reads the config from
  disk, moves leaks, folds a 0.1.6-alpha `secrets.toml` in, and is idempotent —
  the TUI runs it on startup and on project switch. It deliberately leaves
  `user`, `auth.type`, `key_file` and `$ENV` references alone: a project config
  can be a file a team maintains, and deleting lines from it would lose values
  drift never stored for anyone else.
- `applyAccess` is the **read** path, and the project config wins where it
  carries a value, so hand-written files keep working.
- `projectFileBase` re-reads the project config from disk before every write,
  so a write starts from what the file says instead of from the merged,
  defaults-applied, access-filled view in memory. Without it, saving one host
  would bake `[defaults]` into the others and copy this machine's access into a
  file the team shares.
- Global hosts are unaffected: `~/.config/drift/config.toml` is outside every
  repository, so its hosts keep their access fields.

`config.ProjectConfigExposure` (`internal/config/gitguard.go`) reports whether
git can reach the project config; the migration notice uses it to tell the user
that a moved password may still be in the repository's history.

Tests in `internal/config` must never touch the real config directory: its
`TestMain` points `$XDG_CONFIG_HOME` at a temp dir for the whole package,
because the writers reach `access.toml` and a test without that isolation
writes into the developer's own `~/.config/drift`.

Written files: `writeToml` replaces atomically (temp file + rename) and the
encoding-only `hostOut`/`defaultsOut` mirrors exist because BurntSushi's
`omitempty` does not cover numeric zero — drift must trim the files it rewrites,
not decorate them with `port = 0`.

## Logging

`internal/log` is a no-op slog wrapper, **off by default**. It is enabled in
`cmd/root.go` (`runProgram` → `resolveLogConfig` → `log.Init`) when `--log`/`--debug`
or `$DRIFT_LOG`/`$DRIFT_DEBUG` are set; otherwise no file is opened. Never log to
stdout/stderr — the Bubble Tea alt screen owns the terminal; logging goes to a file.
The package is dependency-free: the default path (`<config.Dir()>/drift.log`) is
resolved by the caller in `cmd`, not inside `internal/log`. Use `log.Error` for
failures (with key/value context like `"err"`, path keys) and `log.Info`/`log.Debug`
for lifecycle. slog is concurrency-safe, so logging from `tea.Cmd` goroutines is fine.

## Build & install

```bash
make build                    # go build ./...
make test                     # go test ./...
make vet                      # go vet ./...
make install                  # installs to ~/.local/bin/drift
make update                   # rebuild + reinstall (use after code changes)
make release-build VERSION=vX.Y.Z  # version-injected ./drift binary
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