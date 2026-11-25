# AGENTS.md — drift

Guidelines for AI agents working on this codebase.

## Project overview

drift is a standalone terminal TUI (Go 1.25+ + Bubble Tea, Linux and macOS) for browsing, diffing, and syncing local files with remote hosts over SFTP, FTP, or FTPS. It is inspired by PHPStorm's "Browse Remote Host" / "Sync with Deployed To" workflow.

## Architecture

### Screens & root model

The app is a single Bubble Tea root model (`internal/tui/app.go`) that routes messages to one active screen at a time. Screen transitions happen via typed messages (e.g. `browser.MsgSyncRequested`, `hostselector.MsgHostChosen`). The root model owns all screen models and handles cross-screen messages.

Screens are Go packages under `internal/tui/`:

| Package           | Screen                                                |
| ----------------- | ----------------------------------------------------- |
| `dashboard`       | Project dashboard (optional landing screen)           |
| `projectform`     | Create / edit a project                               |
| `projectselector` | Modal: switch project from the browser (`P`)          |
| `browser`         | File browser (entry point)                            |
| `certtrust`       | Modal: inspect and trust an FTPS certificate          |
| `hostkeytrust`    | Modal: trust the key of an unknown SSH host           |
| `hostselector`    | Modal: pick the sync or remote-pane host (`s`, `@`)   |
| `hostmanager`     | CRUD list of hosts                                    |
| `hostform`        | Create / edit a host (includes mapping sub-screen)    |
| `diffview`        | File list + unified or split diff + sync              |
| `remoteeditor`    | External-editor session + upload confirmation         |

`ScreenRegisterPrompt` (`internal/tui/state.go`) is rendered directly by `app.go` to offer registering an unregistered Git repository.

Shared TUI helper packages under `internal/tui/`:

- `textfield`: single-line input widget used by `hostform` and `projectform`.
- `loading`: progress indicator rendering (consumes `*progress.Tracker`).
- `mouse`: wheel classification, scroll step, and double-click detection (`ClickTracker`).
- `statusbar`: persistent bottom status bar rendering.

The dashboard draws the logo from `internal/logo` (SVG and PNG assets live in `.github/assets/logo/`). Selecting a project on the dashboard or in the project switcher (`P`) re-roots the app via `App.openProject` (`config.Load(path, slug)` + `browser.New(path)`). `P` opens the switcher as a modal and leaves the browser session intact until a different project is chosen.

CLI entry points live in `cmd/`: default `drift` (`--dashboard`, `--no-dashboard`, `--log`, `--debug`), `dash`, `open <name-or-slug>`, `projects` (`list`, `add`, `edit`, `remove`, `archive`), `image-test [image]`, and `version`.

### Application boundaries

Screens handle interaction and rendering only. Building diff sessions lives in `internal/app` (`app.Load`), which returns them sorted as a directory tree for the grouped file list; every refresh and every completed sync rebuilds the comparison through `app.Load`, keeping the scope options. Executing uploads, downloads, and deletes lives in `internal/sync` (`sync.Run`). Neither package imports Bubble Tea or `internal/tui`: they return values and errors, and screens wrap each call in a `tea.Cmd` that turns the result into a typed message. Progress and cancellation cross that boundary as `*progress.Tracker` (`internal/progress`). Do not add `remote.Connect`, path mapping, or transfer loops to a screen package.

`internal/architecture/architecture_test.go` enforces these import and call boundaries over production Go files. Browser connection/listing, host tests, and diff image loading use `app.BrowseRemote`, `app.TestHost`, and `app.LoadImagePair`, not direct screen connections.

Path translation between local and remote is handled by `internal/pathmap`. When effective host-level or project-level fallback mappings are configured, only files within those mappings may be synced. Paths otherwise fall back to `host.RootPath`-relative translation.

### Connections, local roots & keep-alive

Remote I/O goes through the `remote.Client` interface and connection factory (`internal/remote/client.go`). Two implementations exist: `internal/sftp` (SFTP/SSH) and `internal/ftp` (FTP or explicit-TLS FTPS). Use `remote.Connect(ctx, host, trustManager, requiredChallenge)` and pass the root-owned `tlstrust.Manager`; `requiredChallenge` is non-nil only for the first retry after a certificate prompt. Never instantiate protocol clients directly.

The FTP control connection is line-based, so every `internal/ftp` method that takes a path calls `checkPath` before it sends anything; a path with CR, LF, or NUL is a per-file error. Names from a remote listing go through `fs.CheckEntryName` before they are joined onto the listed directory. Because FTP status `550` can also signal permission errors, `internal/ftp`'s `classifyMissing` verifies absence by listing the parent directory (walking up if the parent also returns `550`) and returns `os.ErrNotExist` only when a parent listing succeeds without the exact entry name; `diff.Compare` relies on `errors.Is(err, os.ErrNotExist)` rather than treating every `550` as missing.

Remote clients never receive local paths. Local transfer and mutation operations go through an opened `fs.Root`, which confines them to the project (or the remote editor's separate private working directory) and rejects symlink escapes. Upload with `root.Open` plus `client.Upload`, download with `client.Open` plus `root.WriteAtomic`, and delete local files with `root.Remove`. `WriteAtomic` closes its source and replaces the target only after a complete write. The owner of a successful diff session keeps both the client and root open and closes them together; error paths close resources they created.

Remote clients own their keep-alive monitors. `Host.KeepAliveInterval` is optional: `nil` uses 60 seconds, `0` disables probes. `Done()` closes on normal shutdown or a terminal monitor failure; `Err()` reports only the latter. Keep monitors independent of the short-lived connect context. FTP probes share the operation lock and skip busy connections; SSH probes may run alongside SFTP transfers. Root-owned connection observers must handle failures before modal input routing and reject stale connection/project identities. Browser/diff close methods return commands: detach state in `Update`, run transport shutdown in the returned `tea.Cmd`. Never automatically retry a transfer after connection loss.

### Trust (FTPS certificates & SSH host keys)

FTPS verification and endpoint-scoped exceptions live in `internal/tlstrust`. The root `App` owns the session trust manager. Persistent exceptions live in `<config.Dir()>/trusted-certificates.toml`; `insecure_tls` is not a supported host field. TLS callbacks never block for UI input. They return a typed `tlstrust.VerificationError`, and the root model opens the `certtrust` modal after the async command exits. In that modal, rows drift computed come before text from the certificate, and certificate text is escaped, capped, and indented so it can never start a row.

SSH host keys follow the same pattern in `internal/ssh`. The host key callback never writes and never waits for input: for a host that neither `~/.ssh/known_hosts` nor the session lists it returns a typed `ssh.UnknownHostKeyError` before authentication, and the root model opens the `hostkeytrust` modal. Approval stores the key with `ssh.SaveHostKey` or in `Manager.HostKeys()`; the retry then accepts only that key, so SFTP needs no `requiredChallenge`. A changed key of a known host stays a plain error without a prompt. Root-model code opens either prompt through `App.openTrustPrompt`. Tests that need an SSH host use `internal/sshtest`.

### Diff presentation

Keep display rows, word/syntax spans, and reading snapshots outside `DiffResult` and transfer state. Decoration commands consume immutable comparison lines, never perform I/O, and reject stale request identities; syntax token caching is palette-independent. Global `[ui.diff]` preferences must survive every configuration write and never change sync policy. Refresh restores reading state only, bound to the reload request.

Full layout, continuous-mode, caching, and navigation rules live in [`docs/diff-presentation.md`](docs/diff-presentation.md).

### Terminal images

`cmd` creates one `termimage.Output`, passes it to Bubble Tea with `tea.WithOutput` and to `tui.New`; image transmissions and other escape sequences written outside `View` (such as OSC 52 clipboard copy) take its lock so they never split a frame. Transmit images only from a `tea.Cmd` (`Output.Show`), then render placeholder or half-block rows from `termimage.Rows`; never put raw image data into `View`. Free every placement with `Output.Delete` when its screen moves on or closes, including stale arrivals. Detection is environment-based (`DRIFT_IMAGES`); there is no config key.

Full protocol, decoding, and sizing rules live in [`docs/terminal-images.md`](docs/terminal-images.md).

### Remote editing

Remote editing uses `internal/remoteedit` for scope checks, private cache working copies under `<os.UserCacheDir()>/drift/remote-edit/`, immutable upload snapshots, conflict checks, and cleanup. The `internal/tui/remoteeditor` screen only runs the external process and routes typed results/decisions. It takes exclusive ownership of the existing browser connection and returns it on exit; reconnect/certificate approval only reads and never retries a write. Each session has its own `fs.Root` outside the project. Unconfirmed copies are retained on shutdown; cleanup uses an opened cache-parent `os.Root`, not an unconfined recursive delete. `[editor]` is global user configuration only and must survive every configuration write. Never execute editor commands from project or host records.

## Code conventions

- **Testing** — prefer real connections (`internal/ftptest`, `internal/sshtest`) or skip. Small hand-written stubs for narrow interfaces (like `diff.RemoteClient` in `internal/diff/engine_test.go`) are fine; do not introduce mock frameworks.
- **No speculative abstractions** — add helpers only when used in 3+ places.
- **No backwards-compat shims** — if something is unused, delete it.
- **No error swallowing** — propagate errors to the TUI as typed messages (`MsgDiffError`, session `.Err` field, etc.). Also log connect/sync/diff failures via `internal/log` (see Logging) so they survive past the TUI session.
- **Non-blocking `Update()`** — all async work runs in `tea.Cmd` goroutines.
- **Escape untrusted text** — pass text from external or untrusted sources (file names, remote paths, host fields, error messages, certificate fields) through `styles.SafeText` (`internal/styles/text.go`) before rendering in views.
- **Styles & themes** — semantic palette colors live in `internal/styles/theme.go` (`DRIFT_THEME` = `auto`/`ansi`/`omarchy`/`default`, `DRIFT_THEME_FILE`, Omarchy `colors.toml` lookup), shared styles in `internal/styles/styles.go`, key hints in `internal/styles/key_hints.go` (`KeyHints`/`FitKeyHints`), and root aliases in `internal/tui/styles.go`. Reuse semantic colors (`styles.Color*`) and shared styles; use inline `lipgloss.NewStyle()` in views only for one-off layout/width/border wrappers with palette colors, never hardcoded hex or ANSI colors.
- **Config persistence** — config is TOML (`internal/config/config.go`, `internal/config/writer.go`). Wrap every config or registry write transaction in `config.WithWriteLock`.

## Key types

```go
config.Host          // a remote target (name, hostname, port, auth, root_path, protocol, mappings); Server set = link
config.LinkTarget    // a global server or another project's host a project host can link
config.Mapping       // {Local, Remote} path pair — local relative to project root, remote relative to Host.RootPath
tlstrust.Manager     // session + persistent FTPS certificate trust, plus the session's SSH host keys; owned by tui.App
ssh.HostKeyChallenge // {Address, Key} of an SSH host that known_hosts does not list
fs.Root              // project-confined local Open, Stat, ReadFile, Remove, WriteAtomic
remote.Client        // Stat, ReadDir, Open, ReadFile, Upload, WalkFiles, WalkFilesWithActivity, DeleteFile, Done, Err, Close
diff.Session         // {LocalPath, RemotePath, Result *DiffResult, Err, Loaded}
diff.DiffResult      // comparison output; HasDiff() reports whether files differ
app.LoadRequest      // host, selections, scope options, optional connection and trust for app.Load
progress.Tracker     // progress counters plus the cancellation context of one operation
sync.Item            // {LocalPath, RemotePath, Decision}; sync.Run executes a list of them
diffview.SyncDir     // DirNone / DirUpload / DirDownload / DirDeleteLocal / DirDeleteRemote
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

`internal/fs/local.go` `WalkFiles` skips `.git`, `.svn`, `.hg`, `node_modules`, `.idea`, `.vscode`. Add entries to `skipDirs` for new exclusions — do not add flags or callbacks. The names match without regard to case, because `.GIT` opens `.git` on a case-insensitive filesystem; compare excluded names through `fs.ShouldSkipDir`, never with `==`.

Gitignore classification goes through `fs.Classifier`, which batches paths through `git check-ignore`. The project directory is untrusted input for Git: every call goes through `classifierGitCommand`, which disables `core.fsmonitor` and bare-repository discovery. Do not start Git in the project any other way. Hidden state affects browser visibility only. Gitignored paths are excluded symmetrically from recursive comparisons unless the user includes ignored paths for that operation; directly selected ignored files are exceptions. Hard exclusions can never be overridden. Staging files from interrupted transfers (`fs.StagingName`, recognized by `fs.IsStagingName`) are hard exclusions too; every upload and download names its staging file through `fs.StagingName`. Remote paths must be mapped locally before classification so ignored remote-only files cannot become deletion candidates.

## Configuration & state

drift writes no files into a project's working tree (see [`docs/adr/0001-projektspeicher-nach-slug.md`](docs/adr/0001-projektspeicher-nach-slug.md)). All configuration and runtime state lives outside the repository:

| Purpose          | Path                                                                          |
| ---------------- | ----------------------------------------------------------------------------- |
| Global config    | `~/.config/drift/config.toml` (`$XDG_CONFIG_HOME` overrides `~/.config`)      |
| Project store    | `~/.config/drift/projects/<slug>.toml` (hosts + mappings, mode 600)           |
| Project registry | `~/.config/drift/projects.toml` (via `config.Dir()`)                          |
| FTPS trust       | `~/.config/drift/trusted-certificates.toml` (mode 600)                        |
| Write lock       | `~/.config/drift/write.lock` (mode 600, permanent `flock` lock — never unlink)|
| Log file         | `~/.config/drift/drift.log` (default when `--log`/`--debug` is enabled)       |
| Remote-edit cache| `<os.UserCacheDir()>/drift/remote-edit/` (mode 700, private working copies)   |
| SSH host keys    | `~/.ssh/known_hosts` (written only when a key is trusted permanently)         |

- **Project identity**: `config.Load(root, slug)` takes the slug explicitly because `internal/project` imports `internal/config` for `config.Dir()`. An empty slug means no project, so only global hosts are exposed. Directory-to-project lookup uses `Registry.FindByPathPrefix(dir)` (longest match wins); `cmd/root.go` resolves it in `loadAll` and stores it as `MergedConfig.ProjectSlug`. Unregistered Git repositories trigger the registration prompt (`registerCandidate`, `project.GitRoot`/`SuggestRoot`, `worthStayingIn`, `adoptRegisteredProject`).
- **Server links** ([`docs/adr/0002-server-verknuepfung-ohne-verbindungsfelder.md`](docs/adr/0002-server-verknuepfung-ohne-verbindungsfelder.md)): with a project open, `MergedConfig.Hosts` holds only that project's hosts; global hosts are servers linked via `server = "<name>"` (`internal/config/servers.go`). A link stores only `name`, `server`, `root_path`, and `mappings`; `merge` and `rebuildMerged` resolve it with `config.Resolve`, and `hostsOut` strips connection fields on write. Project `Mappings` are a fallback; host-level `Mappings` take precedence. `config.LinkTargets` scans `projects/` for candidates, `serverUsers` guards `DeleteGlobalHost` and renames in `SaveGlobalHost` (`ServerInUseError`), and `PromoteProjectHost` writes the global config first, then the source store.
- **Locking & atomic writes**: `SaveProjectHost`/`DeleteProjectHost` require a slug and start from `projectStoreBase`, re-reading from disk so a save cannot bake `[defaults]` into other hosts. Every write transaction runs under `config.WithWriteLock` (`internal/config/writelock.go`), which acquires a non-blocking `flock` on `write.lock` and returns `ErrWriteBusy` rather than blocking the UI; never unlink `write.lock`. `writeToml` replaces files atomically (temp file + rename), and the encoding-only `hostOut`/`defaultsOut` mirrors exist because BurntSushi's `omitempty` does not cover numeric zero.
- **Test isolation**: tests in `internal/config` must never touch the real config directory; `TestMain` redirects `$XDG_CONFIG_HOME` to a temp dir, and `isolate(t)` gives individual tests their own directory.

## Logging

`internal/log` is a no-op slog wrapper, **off by default**. It is enabled in `cmd/root.go` (`runProgram` → `resolveLogConfig` → `log.Init`) when `--log`/`--debug` or `$DRIFT_LOG`/`$DRIFT_DEBUG` are set; otherwise no file is opened. Never log to stdout/stderr — the Bubble Tea alt screen owns the terminal; logging goes to a file. The package is dependency-free: the default path (`<config.Dir()>/drift.log`) is resolved by the caller in `cmd`, not inside `internal/log`. Use `log.Error` for failures (with key/value context like `"err"`, path keys) and `log.Info`/`log.Debug` for lifecycle. slog is concurrency-safe, so logging from `tea.Cmd` goroutines is fine.

## Documentation & scripts

| Location                     | Purpose                                                                     |
| ---------------------------- | --------------------------------------------------------------------------- |
| `docs/guide/`                | User guides (`configuration.md`, `keys.md`, `projects.md`, `remote-editing.md`, `sync.md`) |
| `docs/adr/`                  | Architecture Decision Records (`0001`–`0006`; see `docs/adr/README.md`)     |
| `docs/diff-presentation.md`  | Detailed diff rendering, layout, caching, and navigation specification      |
| `docs/terminal-images.md`    | Detailed terminal image protocol, sizing, and lifecycle specification       |
| `docs/manual-testing.md`     | Interactive SFTP/FTPS checklist paired with `scripts/manual-test/mt.sh`     |
| `scripts/screenshots/run.sh` | Regenerates README screenshots in `.github/assets/screenshots/`             |
| `showcase/`                  | Static project showcase website                                             |

`PLAN.md` and `docs/*-plan.md` are completed historical planning documents, not live specifications.

## Before committing

- Run `go test ./...`, `go vet ./...`, and `go build ./...`.
- Add a concise entry under `## [Unreleased]` in `CHANGELOG.md` for user-visible changes (the release workflow extracts release notes from it).
- Update `docs/guide/` or `README.md` when changing user-facing behavior, keybindings, or configuration (and run `scripts/screenshots/run.sh` when the browser, diff view, or dashboard changes noticeably).
- Check that any newly rendered untrusted text goes through `styles.SafeText` and any config/registry writes run inside `config.WithWriteLock`.
- For changes to transfers, connection handling, or the diff view, verify against `docs/manual-testing.md` (`scripts/manual-test/mt.sh`).

## Build & install

```bash
make build                    # go build ./...
make test                     # go test ./...
make vet                      # go vet ./...
make install                  # go install .; uses GOBIN, else GOPATH/bin
make update                   # rebuild + reinstall (use after code changes)
make release-build VERSION=vX.Y.Z  # version-injected ./drift binary
make snapshot                 # all release archives in dist/, nothing published
```

## Releases

GoReleaser (`.goreleaser.yaml`) builds the release archives; `.github/workflows/release.yml` runs it for every pushed `v*` tag. To release:

1. Turn `## [Unreleased]` in `CHANGELOG.md` into `## [X.Y.Z] - YYYY-MM-DD` and merge that into `main`. The workflow takes the release text from this section (`scripts/release-notes.sh`) and fails if it is missing.
2. Tag the merge commit on `main` with `vX.Y.Z` and push the tag.

The workflow runs the tests, builds `drift_<os>_<arch>.tar.gz` for linux and darwin on amd64 and arm64, writes `checksums.txt`, attests the archives, and updates the cask in `WariKoda/homebrew-tap`. Tags with a suffix such as `-alpha` or `-rc.1` become prereleases and leave the tap alone. The tap push needs the repository secret `HOMEBREW_TAP_TOKEN`, a fine-grained token with contents write access to `WariKoda/homebrew-tap`.

`release.mode: keep-existing` means GoReleaser never replaces the text of a release that already exists. To add archives to an old release, check out its tag and run GoReleaser there with the current config (`--config <path to current .goreleaser.yaml>`).

## Git workflow

- Keep `main` stable and releasable; do not work directly on `main` unless the user explicitly asks for it.
- Use a short-lived branch per change (`feature/...`, `fix/...`, `docs/...`, `chore/...`, `refactor/...`) and open a PR into `main` (see [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full flow and PR checklist).
- Merge to `main` only after validation passes; create releases from `main` using tags.

Module path: `github.com/WariKoda/drift`
