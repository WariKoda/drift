# CLAUDE.md — drift-tui

Guidelines for Claude Code working on this codebase.

## Project overview

**drift** is a standalone terminal TUI (Go + Bubble Tea) for browsing, diffing, and syncing local files with remote hosts over SFTP or FTP. Inspired by PHPStorm's "Browse Remote Host" / "Sync with Deployed To" workflow.

Module path: `github.com/nibra180/drift-tui`  
Go version: 1.25+

## Repository structure

```
drift-tui/
├── main.go                     # Package main; calls cmd.Execute()
├── cmd/
│   ├── root.go                 # Cobra CLI; loads config, starts TUI
│   └── version.go              # Version injected at build time via -ldflags
├── internal/
│   ├── config/
│   │   ├── config.go           # Type definitions: Host, Auth, Mapping, MergedConfig
│   │   ├── loader.go           # Discovers and merges global + project configs
│   │   └── writer.go           # Persist config changes (Save/Delete hosts)
│   ├── diff/
│   │   ├── engine.go           # File comparison logic using go-diff
│   │   ├── result.go           # DiffLine, DiffResult, Session types
│   │   └── render.go           # Diff rendering for display
│   ├── fs/
│   │   ├── entry.go            # FileEntry type and SelectionState
│   │   └── local.go            # Directory walking; skips .git, node_modules, etc.
│   ├── ftp/
│   │   └── client.go           # FTP/FTPS connection and file operations
│   ├── pathmap/
│   │   └── mapper.go           # Local ↔ remote path translation (correctness-critical)
│   ├── remote/
│   │   └── client.go           # Protocol abstraction; factory function remote.Connect()
│   ├── sftp/
│   │   └── client.go           # SFTP/SSH connection and file operations
│   ├── ssh/
│   │   ├── auth.go             # Auth method builders: keyfile, password, agent
│   │   └── knownhosts.go       # Known hosts verification
│   ├── styles/
│   │   └── styles.go           # Centralized lipgloss color and style definitions
│   ├── sync/
│   │   └── plan.go             # Sync plan and progress types
│   └── tui/
│       ├── app.go              # Root Bubble Tea model; screen routing
│       ├── state.go            # Screen constants and AppState type
│       ├── styles.go           # Style convenience aliases
│       ├── browser/            # File browser screen (model.go, update.go, view.go, tree.go, keys.go)
│       ├── diffview/           # Diff view + sync direction control (model.go, update.go, view.go)
│       ├── hostform/           # Create/edit host + mappings (model.go, update.go, view.go, textfield.go)
│       ├── hostmanager/        # CRUD host list (model.go, update.go, view.go)
│       ├── hostselector/       # Modal host picker (model.go)
│       └── statusbar/          # Status bar (view.go)
├── testdata/
│   ├── config_global.toml      # Example global config
│   └── config_project.toml     # Example project config
├── Makefile
├── go.mod / go.sum
├── README.md                   # User-facing docs
├── PLAN.md                     # Implementation plan (historical reference)
└── AGENTS.md                   # Additional AI agent guidelines
```

## Application flow

```
main.go → cmd.Execute()
  └─ cmd/root.go
       ├─ config.Load(workDir)   # merge ~/.config/drift/config.toml + .drift/config.toml
       └─ tui.New(workDir, cfg)  # create root model
            └─ tea.NewProgram(app).Run()
```

**Screen state machine:**

```
ScreenBrowser (initial)
  ├─ [s] with selection → ScreenHostSelector
  │     └─ [Enter] → ScreenDiffLoading → ScreenDiffView
  │           ├─ [Space] cycle sync direction per file
  │           ├─ [A] cycle all files' sync direction
  │           ├─ [s] sync current file
  │           ├─ [S] sync all files → ScreenSyncProgress
  │           └─ [Esc/q] → ScreenBrowser
  └─ [h] → ScreenHostManager
        ├─ [n/e] → ScreenHostForm → ScreenHostManager
        └─ [Esc] → ScreenBrowser
```

Screen transitions are driven by typed messages (e.g. `browser.MsgSyncRequested`, `hostselector.MsgHostChosen`). The root model in `app.go` owns all screen models.

## Architecture

### Protocol abstraction

All remote I/O goes through `remote.Client` (`internal/remote/client.go`). Two implementations:
- `internal/sftp` — SFTP over SSH
- `internal/ftp` — FTP/FTPS

**Always use `remote.Connect(ctx, host)`** — never instantiate protocol clients directly.

### Path mapping

`internal/pathmap/mapper.go` translates local ↔ remote paths. When a host has `Mappings`, only files within those mappings can be synced. Falls back to `host.RootPath`-relative paths when no mappings are set. This package is correctness-critical — changes here affect all sync operations.

### Async I/O

All network I/O runs in `tea.Cmd` goroutines. **Never block `Update()`**. Errors are sent back to the TUI as typed messages (e.g. `MsgDiffError`, session `.Err` field).

### Styling

All lipgloss styles are defined in `internal/styles/styles.go` and `internal/tui/styles.go`. **Do not inline lipgloss styles in view code.**

## Key types

```go
config.Host         // remote target: name, hostname, port, auth, root_path, protocol, mappings
config.Mapping      // {Local, Remote} path pair — relative local, absolute remote
config.Auth         // {Type, KeyFile, Password} — auth method for SSH/SFTP

remote.Client       // interface: Stat, ReadFile, WriteFile, UploadFile, DownloadFile,
                    //            WalkFiles, DeleteFile, Close

diff.Session        // {LocalPath, RemotePath, Result *DiffResult, Err, Loaded}
diff.DiffResult     // comparison output; HasDiff() reports whether files differ

diffview.SyncDir    // DirNone / DirUpload / DirDownload / DirDeleteLocal / DirDeleteRemote

fs.FileEntry        // local file with path, size, mtime, and SelectionState
```

## Configuration

| Scope   | Path |
|---------|------|
| Global  | `~/.config/drift/config.toml` (or `$XDG_CONFIG_HOME/drift/config.toml`) |
| Project | `.drift/config.toml` in project root (walked up from cwd) |

Project hosts override global hosts by the same name. Host-level `Mappings` take precedence over project-level `Mappings`.

**Config types** live in `internal/config/config.go`. **Persistence** via `internal/config/writer.go`. Format is TOML — see `testdata/` for examples.

## Build & development

```bash
go build -o drift .          # build local binary
make install                  # build and install to ~/.local/bin/drift
make update                   # rebuild + reinstall (use after code changes)

# Version injection at build time:
go build -ldflags "-X github.com/nibra180/drift-tui/cmd.Version=x.y.z" -o drift .
```

No test suite exists. `testdata/` contains example config files for manual testing.

## Code conventions

- **No mocks in tests** — use real connections or skip.
- **No speculative abstractions** — add helpers only when used in 3+ places.
- **No backwards-compat shims** — if something is unused, delete it completely.
- **No error swallowing** — propagate errors as typed TUI messages.
- **No blocking in Update()** — all I/O belongs in `tea.Cmd` goroutines.
- **No inline styles** — all lipgloss styles go in `internal/styles/` or `internal/tui/styles.go`.

## Adding a new TUI screen

1. Create `internal/tui/<name>/model.go`, `view.go`, `update.go`.
2. Define typed entry/exit messages in the package.
3. Add the model as a field on `tui.App` (`internal/tui/app.go`).
4. Add a `Screen<Name>` constant to `internal/tui/state.go`.
5. Handle entry/exit messages and delegate `Update`/`View` in `app.go`.

## Adding a new protocol

1. Implement `remote.Client` in a new package under `internal/`.
2. Add a protocol constant to `config.Host.Protocol`.
3. Add a case to `remote.Connect()` in `internal/remote/client.go`.
4. Add a toggle to `hostform` (`fProtocol` field in `model.go`, `visibleRows`, `view.go`).

## File walker exclusions

`internal/fs/local.go` `WalkFiles` skips: `.git`, `.svn`, `.hg`, `node_modules`, `.idea`, `.vscode`.

Add new exclusions to the `skipDirs` map — do not add flags or callbacks.

## Critical files

Changes to these files have broad impact — be careful and test manually:

| File | Why it's critical |
|------|-------------------|
| `internal/config/loader.go` | Everything depends on config loading |
| `internal/tui/app.go` | Root state machine; controls all screen transitions |
| `internal/tui/diffview/model.go` | Most complex component |
| `internal/pathmap/mapper.go` | Correctness of all sync operations |
| `internal/remote/client.go` | Protocol abstraction interface |

## SSH authentication

Auth types (`config.Auth.Type`): `keyfile`, `password`, `agent`

- **keyfile**: Reads private key file; supports passphrase-protected keys.
- **password**: Supports env var expansion (e.g. `$DEPLOY_PASSWORD`).
- **agent**: Connects to SSH agent socket (`$SSH_AUTH_SOCK`).

Known hosts verification is handled in `internal/ssh/knownhosts.go`. Connection timeout: 15 seconds.

## Dependencies

```
charmbracelet/bubbletea v1.3.10   # TUI framework
charmbracelet/lipgloss v1.1.0     # Terminal styling
BurntSushi/toml v1.6.0            # TOML parsing
spf13/cobra v1.10.2               # CLI framework
pkg/sftp v1.13.10                 # SFTP protocol
jlaffaye/ftp v0.2.0               # FTP protocol
sergi/go-diff v1.4.0              # Diff algorithm
golang.org/x/crypto v0.49.0       # SSH/crypto
```
