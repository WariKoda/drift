# drift — Implementation Plan

Terminal-based remote file sync tool. Run `drift` in any directory to browse, diff, and sync files with a remote host over SFTP/SSH.

---

## User Flow

```
drift (run in project dir)
  └─► File Browser (yazi-like)
        └─► [Space] mark files/dirs  [s] sync
              └─► Host Selector (modal)
                    └─► [Enter] select host
                          └─► Split Diff View (local | remote)
                                └─► [u] upload  [d] download  [U/D] sync all
                                      └─► Sync Progress View
                                            └─► back to Browser
```

---

## Project Structure

```
drift/
├── go.mod
├── go.sum
├── main.go
├── cmd/
│   ├── root.go          # cobra entry, launches TUI
│   └── version.go
├── internal/
│   ├── config/
│   │   ├── config.go    # types (Host, Mapping, Auth, ...)
│   │   ├── loader.go    # find + merge global & project configs
│   │   └── validator.go # friendly error messages for bad config
│   ├── tui/
│   │   ├── app.go       # root bubbletea Model — state machine
│   │   ├── state.go     # Screen enum + AppState struct
│   │   ├── keys.go      # global keybindings
│   │   ├── styles.go    # all lipgloss styles
│   │   ├── browser/
│   │   │   ├── model.go   # file browser
│   │   │   ├── update.go
│   │   │   ├── view.go
│   │   │   ├── tree.go    # directory tree data structure
│   │   │   └── keys.go
│   │   ├── hostselector/
│   │   │   ├── model.go   # fuzzy host picker (modal)
│   │   │   └── view.go
│   │   ├── diffview/
│   │   │   ├── model.go   # split-pane diff view
│   │   │   ├── update.go
│   │   │   ├── view.go
│   │   │   └── keys.go
│   │   └── statusbar/
│   │       └── view.go
│   ├── fs/
│   │   ├── entry.go     # FileEntry type (local & remote)
│   │   └── local.go     # local filesystem walker
│   ├── ssh/
│   │   ├── client.go    # SSH connection
│   │   ├── pool.go      # connection reuse
│   │   └── auth.go      # password / keyfile / agent
│   ├── sftp/
│   │   ├── client.go    # SFTP client wrapper
│   │   ├── walker.go    # remote directory tree walk
│   │   └── transfer.go  # upload/download with progress
│   ├── diff/
│   │   ├── engine.go    # compare local vs remote file
│   │   ├── result.go    # DiffResult, DiffLine types
│   │   └── render.go    # colored side-by-side rendering
│   ├── sync/
│   │   ├── engine.go    # sync orchestration
│   │   ├── plan.go      # build SyncPlan from selection
│   │   └── progress.go  # progress tracking + tea.Cmd wrappers
│   └── pathmap/
│       └── mapper.go    # local ↔ remote path translation
└── testdata/
    ├── config_global.toml
    └── config_project.toml
```

---

## Configuration

### Global: `~/.config/drift/config.toml`

```toml
[defaults]
port = 22
user = "deploy"

[[hosts]]
name = "staging"
hostname = "staging.example.com"
user = "deploy"
root_path = "/var/www/staging"

  [hosts.auth]
  type = "keyfile"
  key_file = "~/.ssh/id_ed25519"

[[hosts]]
name = "prod"
hostname = "prod.example.com"
port = 2222
user = "admin"
root_path = "/srv/app"

  [hosts.auth]
  type = "keyfile"
  key_file = "~/.ssh/id_rsa"
  passphrase = "$SSH_KEY_PASSPHRASE"   # env var expansion

[[hosts]]
name = "devbox"
hostname = "192.168.1.50"
user = "vagrant"
root_path = "/home/vagrant/project"

  [hosts.auth]
  type = "password"
  password = "$DEVBOX_PASS"
```

### Project: `.drift/config.toml`

```toml
# Same-name hosts override global hosts for this project
[[hosts]]
name = "staging"
hostname = "staging-myproject.example.com"
root_path = "/var/www/myproject"

  [hosts.auth]
  type = "keyfile"
  key_file = "~/.ssh/myproject_deploy"

# local path (rel. to project root) → remote path (rel. to host root_path)
[[mappings]]
local  = "src/"
remote = "app/src/"

[[mappings]]
local  = "public/"
remote = "app/public/"

[[mappings]]
local  = "config/nginx.conf"
remote = "etc/nginx/sites-enabled/myproject.conf"
```

---

## Data Types

### config.go

```go
type Host struct {
    Name     string
    Hostname string
    Port     int      // default 22
    User     string
    Auth     Auth
    RootPath string
}

type Auth struct {
    Type       string  // "password" | "keyfile" | "agent"
    Password   string  // supports $ENV_VAR
    KeyFile    string  // ~ expanded
    Passphrase string  // supports $ENV_VAR
}

type Mapping struct {
    Local  string
    Remote string
}
```

### fs/entry.go

```go
type FileEntry struct {
    Name     string
    Path     string
    Kind     EntryKind  // File | Dir | Symlink
    Size     int64
    ModTime  time.Time
    Depth    int
    Expanded bool
    Children []*FileEntry
    Parent   *FileEntry
}
```

### diff/result.go

```go
type DiffResult struct {
    LocalPath   string
    RemotePath  string
    Lines       []DiffLine
    LocalOnly   bool   // file missing on remote
    RemoteOnly  bool   // file missing locally
    Binary      bool
    SizeLocal   int64
    SizeRemote  int64
    ModLocal    time.Time
    ModRemote   time.Time
}

type DiffLine struct {
    LocalLine  string
    RemoteLine string
    Kind       LineKind   // Equal | Added | Removed | Modified
    LocalNum   int
    RemoteNum  int
}
```

### tui/state.go

```go
type Screen int

const (
    ScreenBrowser Screen = iota
    ScreenHostSelector
    ScreenDiffView
    ScreenSyncProgress
)

type AppState struct {
    Screen        Screen
    WorkingDir    string
    Config        *config.MergedConfig
    Tree          *browser.TreeModel
    Selection     *fs.SelectionState    // marked paths
    SelectedHost  *config.Host
    DiffSessions  []diff.Session
    ActiveSession int
    SyncPlan      *sync.Plan
    SyncProgress  *sync.Progress
    StatusMsg     string
    StatusKind    StatusKind
    TermWidth     int
    TermHeight    int
}
```

---

## TUI State Machine

```
ScreenBrowser
  [s with selection] ──► ScreenHostSelector
                               [Enter] ──► ScreenDiffView  (async diff load)
                                               [u/d] ──► ScreenSyncProgress
                                               [U/D] ──► ScreenSyncProgress
                                                              [done/q] ──► ScreenBrowser
                                               [q/Esc] ──► ScreenBrowser
                               [Esc] ──► ScreenBrowser
```

Messages driving transitions:
```go
type MsgScreenChange  struct{ To Screen }
type MsgHostSelected  struct{ Host config.Host }
type MsgDiffLoaded    struct{ Sessions []diff.Session }
type MsgDiffError     struct{ Err error }
type MsgSyncTick      struct{ Progress sync.Progress }
type MsgSyncDone      struct{}
```

All network I/O runs as `tea.Cmd` (goroutines), never blocking the render loop.

---

## Key Bindings

### Browser

| Key | Action |
|-----|--------|
| `j/k` `↓/↑` | Move cursor |
| `h` `←` | Collapse dir / go to parent |
| `l` `→` `Enter` | Expand dir |
| `Space` | Toggle mark |
| `v` | Visual selection mode |
| `V` | Select all in dir |
| `*` | Invert selection |
| `s` | Open host selector |
| `r` | Refresh |
| `/` | Filter |
| `Esc` | Clear filter / selection |
| `g` / `G` | Top / bottom |
| `?` | Help overlay |
| `q` | Quit |

### Host Selector (modal)

| Key | Action |
|-----|--------|
| Type | Fuzzy filter |
| `j/k` `↓/↑` | Navigate |
| `Enter` | Select host → load diff |
| `Esc` | Cancel |

### Diff View

| Key | Action |
|-----|--------|
| `j/k` `↓/↑` | Scroll diff |
| `Tab` / `Shift+Tab` | Next / prev file |
| `u` | Upload current file |
| `d` | Download current file |
| `U` | Upload ALL |
| `D` | Download ALL |
| `[` / `]` | Jump to prev/next diff hunk |
| `c` | Toggle context lines (3 / full) |
| `?` | Help |
| `q` `Esc` | Back to browser |

### Sync Progress

| Key | Action |
|-----|--------|
| `q` `Esc` | Abort transfers |
| (auto-returns on completion) | |

---

## UI Mockups

### Browser

```
  drift  ~/Work/myproject                      [staging] [prod]
  ──────────────────────────────────────────────────────────────
  ▼ src/
  │  ● Controller.php                          marked
  │  ● Service.php                             marked
  │  ▶ utils/                                  (collapsed)
  ▼ public/
  │    index.html
  │    style.css
    config/
       nginx.conf

  2 files marked  [s] sync  [Space] mark  [?] help
```

### Host Selector (modal overlay)

```
  ┌─ Select host ──────────────────┐
  │  > prod_                       │
  │  ──────────────────────────── │
  │    prod    prod.example.com    │
  │    staging staging.example.com │
  └────────────────────────────────┘
```

### Diff View

```
  drift  src/Controller.php  (2/2)           [staging] prod.example.com
  ──────────────────────────────────────────────────────────────────────
  LOCAL                          │  REMOTE
  ─────────────────────────────  │  ────────────────────────────────────
   1  <?php                      │   1  <?php
   2  class Controller {         │   2  class Controller {
   3  ┄┄┄ @@ -12,7 +12,9 @@ ┄┄┄ │   3  ┄┄┄ @@ -12,7 +12,9 @@ ┄┄┄
  12    function index() {       │  12    function index() {
  13      $data = fetch();       │  13      $data = fetch();
  14 ▓▓   return view($data);    │  14      return view($data, true);  ◀ remote
  15    }                        │  15    }

  [u] upload  [d] download  [U] upload all  [D] download all  [q] back
```

---

## Implementation Phases

### Phase 1 — Foundation
- `go.mod` setup, all dependencies
- `internal/config`: types, TOML loading, global+project merge, env expansion
- `internal/pathmap`: mapping resolution
- `cmd/root.go`: cobra, reads working dir, loads config
- Unit tests for config merge and path mapper

### Phase 2 — Local File Browser
- `internal/fs/entry.go` + `local.go`
- `internal/tui/browser`: tree model, navigation, expand/collapse, multi-select
- `internal/tui/styles.go`, `statusbar/`
- `internal/tui/app.go` wiring browser as initial screen

### Phase 3 — Remote + Diff
- `internal/ssh/` + `internal/sftp/client.go` + `walker.go`
- `internal/diff/engine.go` + `render.go`
- `internal/tui/hostselector/`
- `internal/tui/diffview/`
- Wire full flow: Browser → HostSelector → DiffView
- Async diff loading with spinner

### Phase 4 — Sync Engine
- `internal/sftp/transfer.go` with progress callbacks
- `internal/sync/plan.go` + `engine.go` + `progress.go`
- Sync progress view
- Context cancellation (abort mid-transfer)
- `sftp.MkdirAll` for missing remote parent dirs

### Phase 5 — Polish
- SSH connection pool (`internal/ssh/pool.go`)
- Binary file detection
- Symlink handling
- `~/.ssh/known_hosts` verification
- Help overlays
- Config validator with friendly errors
- Version injection via `-ldflags`

---

## Go Module (`go.mod`)

```
module github.com/yourusername/drift

go 1.22

require (
    github.com/charmbracelet/bubbletea  v0.27.x
    github.com/charmbracelet/lipgloss   v0.13.x
    github.com/charmbracelet/bubbles    v0.20.x
    github.com/BurntSushi/toml          v1.4.x
    github.com/spf13/cobra              v1.8.x
    github.com/pkg/sftp                 v1.13.x
    github.com/sergi/go-diff            v1.3.x
    golang.org/x/crypto                 v0.28.x
    golang.org/x/term                   v0.25.x
)
```

---

## Critical Files (start here)

1. `internal/config/loader.go` — everything depends on this
2. `internal/tui/app.go` — root state machine
3. `internal/tui/diffview/model.go` — most complex component
4. `internal/sync/engine.go` — sync with cancellation
5. `internal/pathmap/mapper.go` — correctness critical for all sync ops
