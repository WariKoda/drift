# drift

A terminal TUI for browsing, diffing, and syncing files with remote hosts — think PHPStorm's "Browse Remote Host" and "Sync with Deployed To", but in your terminal.

Supports **SFTP/SSH** and **FTP** targets. Runs on Linux and macOS.

---

## Features

- File browser with multi-select (Space) and recursive directory marking
- Side-by-side diff view for local vs. remote files
- Per-file sync direction control: upload ↑, download ↓, delete local ✗, delete remote ✗, or skip —
- Bulk sync direction toggle (A key cycles all files at once)
- Auto pre-selection of sync direction based on file modification time
- Sync current file (s) or all marked files (S) in one keystroke
- Per-host path mappings (like PHPStorm's Deployment Mappings tab)
- Host manager: create, edit, delete, and test connections
- Global config (`~/.config/drift/config.toml`) + project-level config (`.drift/config.toml`)
- Skips `.git`, `node_modules`, `.idea`, and other irrelevant directories automatically

---

## Installation

### From source (requires Go 1.21+)

```bash
git clone https://github.com/nibra180/drift-tui.git
cd drift-tui
make install
```

This builds the binary and installs it to `~/.local/bin/drift`.

### Update after code changes

```bash
make update
```

---

## Usage

```bash
# Start in the current directory
drift

# Show version
drift version
```

Navigate to any file or directory, press **Space** to mark it, then **s** to open the sync target picker.

---

## Key Bindings

### File Browser

| Key | Action |
|-----|--------|
| `j` / `k` or `↑` / `↓` | Navigate |
| `Space` | Mark / unmark file or directory |
| `Enter` | Open directory |
| `Backspace` | Go up one level |
| `s` | Sync marked files (opens host selector) |
| `h` | Open host manager |
| `q` / `Esc` | Quit |

### Diff View

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate file list |
| `J` / `K` | Scroll diff content |
| `Space` | Cycle sync direction for current file |
| `A` | Cycle sync direction for all files |
| `s` | Sync current file |
| `S` | Sync all files |
| `r` | Refresh diffs |
| `Esc` | Back to browser |

### Host Manager

| Key | Action |
|-----|--------|
| `n` | New host |
| `e` / `Enter` | Edit host |
| `d` | Delete host |
| `t` | Test connection |
| `Esc` | Back |

---

## Configuration

### Global config: `~/.config/drift/config.toml`

```toml
[defaults]
user = "deploy"

[[hosts]]
name       = "prod"
hostname   = "example.com"
port       = 22
user       = "deploy"
root_path  = "/var/www/html"
protocol   = "sftp"

  [hosts.auth]
  type     = "keyfile"
  key_file = "~/.ssh/id_ed25519"
```

### Project config: `.drift/config.toml`

Place this file in your project root. drift walks up from the working directory to find it.

```toml
[[hosts]]
name      = "staging"
hostname  = "shopdev.example.com"
port      = 21
user      = "webuser"
root_path = "/var/www"
protocol  = "ftp"

  [hosts.auth]
  type     = "password"
  password = "$DEPLOY_PASSWORD"

  [[hosts.mappings]]
  local  = "plugins/plugin1"
  remote = "/var/www/html/custom/plugins/plugin1"

  [[hosts.mappings]]
  local  = "plugins/plugin2"
  remote = "/var/www/html/custom/plugins/plugin2"
```

### Path Mappings

When a host has `mappings` configured, only files that fall under a mapping rule can be synced. Files outside all mappings are excluded. Without mappings, all files sync relative to `root_path`.

### Auth types

| Type | Fields |
|------|--------|
| `keyfile` | `key_file`, `passphrase` (optional) |
| `password` | `password` (supports `$ENV_VAR`) |
| `agent` | none — uses SSH agent |

---

## Project Structure

```
internal/
  config/       config types, loader, writer
  diff/         diff engine, result types, renderer
  ftp/          FTP client (jlaffaye/ftp)
  fs/           local file walker, directory reader
  pathmap/      local ↔ remote path resolution with mapping rules
  remote/       protocol-agnostic Client interface
  sftp/         SFTP/SSH client
  sync/         sync plan types
  tui/
    app.go      root Bubble Tea model, screen routing
    browser/    file browser screen
    diffview/   diff + sync screen
    hostform/   host create/edit form (incl. mapping manager)
    hostmanager/host list screen
    hostselector/sync target picker
    styles.go   shared lipgloss styles
```

---

## License

MIT
