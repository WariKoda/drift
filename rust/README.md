# drift-gui

See the [Rust/GPUI port plan](../docs/rust-port-plan.md) for milestones and the
planned System/Dark/Light modes with Monokai Pro Dark and Monokai Pro Light Sun.

The Rust desktop application develops alongside the Go TUI. It currently provides
a local browser with directory navigation, filtering, a project-wide finder and a
read-only UTF-8 text preview (up to 1 MiB). The Projects panel opens registered
projects or registers the current folder. Hosts manages project targets and global
servers with forms, duplication, deletion, server links and mappings.
SFTP browsing and preview are implemented. Comparison/sync and FTP/FTPS
connections remain pending.

## Standalone applications

The Go TUI and Rust GUI remain independently maintained products in one repository.
Each has its own executable, build dependencies, installation, version and release
cycle. The TUI installs as `drift`, the GUI as `drift-gui`; neither requires the
other executable. Rust implements its own core instead of wrapping Go.

Both use the existing configuration directory, registry, project hosts and mappings.
Stored records remain separate from resolved defaults and server connections.
The updated Go implementation and Rust store coordinate management operations with
`flock` on `<config.Dir()>/write.lock`. The lock spans fresh reads, validation and
all writes. A stale edit of the same record conflicts; unrelated registry edits
are merged. The lock file is never removed. Contention returns a visible retry
error instead of waiting on the UI thread.

Use the Go TUI built from this branch when running both applications against shared
configuration. Older Go installations do not participate in the common transaction
lock. GUI window/theme/pane preferences will live separately in `gui.toml`; that
persistence is not implemented yet. Shared file format changes still require
coordination despite independent application versions.

## Build and run

Rust is pinned in `rust-toolchain.toml`; Cargo.lock fixes GPUI Kit and its matching
GPUI packages together. From the repository root:

```sh
make rust-check
make rust-test
make rust-build
make rust-run
# Browse a particular folder:
cd rust && cargo run --locked -p drift-gui -- /path/to/project
# Optional: install the GUI into Cargo's binary directory.
make rust-install
```

The GUI starts in the current directory. A registered containing project supplies
its capability root and hosts; the longest registered path wins. Up stays within
registered projects. For unregistered folders, Up opens the parent with a new
capability root. Open folder uses the native folder chooser. Back and Forward
follow successfully loaded directories; a failed load keeps the current folder.
Click a directory to enter it or a file to preview it in the opposite pane.
The finder searches the entire project; the filter narrows the
returned paths. Hidden and ignored entries have separate visibility toggles.
Fixed exclusions and interrupted transfer staging files remain excluded.

Preview uses Kit's read-only text control with line numbers, wrapping and native
text selection/copy. Copy text copies the complete preview; Copy path copies the
selected project-relative path. Ctrl+F/Cmd+F focuses the file filter, F5 refreshes,
and Cancel stops the active listing/finder/preview. In the browser, arrows or
J/K move the selection, Enter/Right/L opens it, and Backspace/Left/H goes up.
Alt+Left/Right follows history and Alt+Up goes up. These browser shortcuts leave
text inputs free for typing. Projects has a name/slug filter and registers the
displayed folder. Closing Projects keeps the browser session and restores focus.
Directory and preview responses carry separate generations;
stale responses are discarded and their root handles released.

Use **Remote** in the toolbar, then **Connect <host>**, to open an SFTP target
beside the local browser. Each side keeps its own path, filter, selection, history,
scroll and focus. Remote Back/Forward/Up and the browser keys navigate within the
configured host root. Remote text preview opens in the left pane; **Local files**
restores the local browser. Selecting a local file restores the right preview;
**Remote** returns to the connected remote tree. **Disconnect** closes it.

SFTP supports password, key file (including passphrase) and SSH-agent authentication.
Password, passphrase and key path expand environment variables; `~/` expands in
key paths. Empty auth/key-file settings use `SSH_AUTH_SOCK`. Connect/auth is bounded
by 15 seconds. Unknown host keys are added to `~/.ssh/known_hosts` following Go's
TOFU behavior. Hashed entries, OpenSSH patterns, preferred known host-key algorithms
and revocation are checked; changed keys fail visibly. Host certificates/CA entries
are currently rejected explicitly and still need parity work.

Keep-alive belongs to the connection: default 60 seconds, zero disables it and
missing replies time out after 15 seconds. Connection failures are observed even
while Hosts is open. Project changes, changed host configuration and window closure
cancel/close remote resources. Explicitly cancelling active remote I/O closes that
connection; reconnect is explicit. Replacing a preview lets its bounded read finish
and close the file handle, discards the old result, then reads the latest selection.
Only one preview reads at a time per session, with a 15-second deadline. FTP/FTPS targets currently show
an unsupported-connection error.

Hosts opens a separate management view and keeps the browser session. Project
hosts and global servers have separate lists; links use an existing global server
and store only their name, server, root path and mappings. Connection forms support
SFTP, FTP and explicit FTPS, password/key file/SSH-agent settings, and optional
keep-alive (empty means 60 seconds, zero disables probes). Passwords/passphrases
are masked. Empty port/user fields keep the stored defaults instead of saving
resolved values. Local/deploy mapping paths remain relative to their roots.

Save conflicts and validation errors leave the form open. Cancel it, reload the
list and reopen the record to use a newer version. Delete asks for confirmation;
deleting or renaming a server used by project links fails visibly. Closing Hosts
restores browser focus. Management I/O runs on the bounded background pool;
completed changes refresh the current project's resolved configuration without
resetting its file list or preview. Connection testing and linking hosts from other
projects via server promotion remain pending.

Filesystem and Git work run outside rendering on a bounded Tokio/background pool.
Git itself classifies ignored paths in batches, including tracked files and
worktree/global ignore rules. Outside a repository it uses temporary bare metadata
outside the project. Local I/O uses cap-std directory capabilities, not a
canonicalize-then-open check. Preview refuses escapes, FIFOs, special files, NUL
bytes, invalid UTF-8 and oversized files. Filenames that cannot be displayed as
UTF-8 currently produce an error rather than being silently renamed.

Linux needs a Wayland or X11 session and a Vulkan driver. On Ubuntu 24.04:

```sh
sudo apt-get install gcc g++ clang pkg-config libfontconfig-dev libwayland-dev \
  libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev libvulkan1
```

macOS needs macOS 15+ and Xcode Command Line Tools. Set
`MACOSX_DEPLOYMENT_TARGET=15.0` for local release builds as CI does. See the
[GPUI Kit installation guide](https://gpui-kit.com/docs/installation/).
`make rust-build` creates `rust/target/release/drift-gui`; Go build targets remain
independent.

## Architecture and verification

- `drift-core`: raw TOML types, runtime resolution, transactional stores, registry,
  mapping and capability-confined local operations. Atomic local writes use drift
  staging names, preserve target permissions, check source completion, flush/sync
  and explicit close before rename.
- `drift-app`: background local listing, batched Git classification, finder and
  preview, bounded workers and cancellation. No GPUI dependency.
- `drift-gui`: Kit controls, view state, operation identities and clipboard actions.
  Views do not perform filesystem or network I/O.

The window's `shell.rs` coordinates project changes, registration and host
management through typed child-view events. Each `BrowserPane` in `browser.rs`
owns its navigation/history, selection, filter, finder, focus, scroll and listing
cancellation. `preview.rs` owns its editor and separate request lifetime;
`projects.rs` owns project-panel inputs. `toolbar.rs` renders stateless controls
and `actions.rs` defines key bindings. `remote.rs` owns the remote pane and its
connection/listing identities. Unified diff will gain its own view.

Shared fixtures in `../testdata/parity/` verify Go/Rust mapping and staging policy.
Rust tests use real temporary trees, symlinks, a FIFO, Git processes, transaction
locks and failed completion operations. GPUI tests use real file listings in a
headless test window, including nested navigation, project boundaries, input focus,
failed loads, registration, stale-result rejection and picker/session lifetime.
Two panes in one headless window verify independent filter, selection, focus,
history and cancellation; preview tests verify replacement and clipboard content.
SSH/SFTP tests start a real Go daemon from `../testdata/sftp-server/` against
isolated temporary trees and use real OpenSSH keys/agents. Go, `ssh-keygen`,
`ssh-agent` and `ssh-add` are required for `cargo test`; production builds and the
GUI executable are native Rust. Tests include authentication, hashed/pattern/revoked
host keys, key changes, agent and keep-alive timeout, connection loss, scope,
remote navigation and cross-view preview/clipboard. No protocol tests are skipped.
Host tests use real stores to verify forms, CRUD, duplication, defaults, links,
masked fields, concurrent-edit conflicts and deletion guards.

Cross-process Go/Rust store tests run in Linux and macOS CI. To run them locally:

```sh
cd rust && cargo build --locked -p drift-core --example store_probe
cd ..
DRIFT_RUST_STORE_PROBE="$PWD/rust/target/debug/examples/store_probe" go test ./internal/parity
```

All test stores use temporary directories. Native Wayland, X11, macOS Intel and
Apple Silicon rendering/OS clipboard checks remain manual: navigate a temporary
project, use the native folder chooser, navigate back and forward, filter and find
files, exercise host forms and return to the browser, preview/copy text into another
app, toggle hidden
and ignored paths, change projects during loading, close the picker, and close the
window. Headless tests cannot establish native rendering or OS clipboard behavior.

## Remaining port work

Milestone 2 is in progress: server promotion/endpoint link offers, project edit/delete
and dashboard/startup restoration, GUI preferences and full certificate-store
roundtrip coverage remain. SFTP transport/browser is available;
its comparison/unified diff and serial sync remain to be implemented, as do
FTP/FTPS, certificate challenges, complete CLI and
keyboard parity, packaging and native release acceptance. Safe atomic-write
primitives are tested; there is no upload/download/delete sync implementation yet.
Blocking local filesystem calls already running cannot be interrupted by Tokio; their eventual
results are discarded after cancellation and concurrency remains bounded.
