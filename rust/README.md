# drift-gui

See the [Rust/GPUI port plan](../docs/rust-port-plan.md) for milestones and the
planned System/Dark/Light modes with Monokai Pro Dark and Monokai Pro Light Sun.

The Rust desktop application develops alongside the Go TUI. It currently provides
a local browser with directory navigation, filtering, a project-wide finder and a
read-only UTF-8 text preview (up to 1 MiB). The Projects panel opens registered
projects or registers the current folder. Hosts manages project targets and global
servers with forms, duplication, deletion, server links and mappings. A separate
picker can promote a host from another project into a shared global server.
SFTP, FTP and explicit FTPS browsing, preview, unified comparison and serial sync
are implemented. FTPS certificate challenges offer session or permanent trust.
Hosts can test saved targets or unsaved forms and reset endpoint-specific FTPS
certificate exceptions without replacing the active browser connection.

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

Use **Remote** in the toolbar, then **Connect <host>**, to open an SFTP, FTP or FTPS target
beside the local browser. Each side keeps its own path, filter, selection, history,
scroll and focus. Remote Back/Forward/Up and the browser keys navigate within the
configured host root. Remote text preview opens in the left pane; **Local files**
restores the local browser. Selecting a local file restores the right preview;
**Remote** returns to the connected remote tree. **Disconnect** closes it.

After connecting, **Compare project** compares the project (or all effective mapping
roots). **Compare local selection** and **Compare remote selection** compare the
selected file, or the current folder when no row is selected. Directories expand
both counterparts recursively, including files that exist on only one side.
Hidden visibility does not narrow comparison scope. **Include ignored** is a
separate operation setting; directly selected ignored files remain exceptions,
while fixed exclusions and transfer staging files never enter the comparison.

The comparison shows differing files and per-file errors, with suggested actions.
Click the action button or press Enter/Space in the file list to cycle valid
previews. Upload shows Remote → Local, Download Local → Remote and deletion shows
the affected side being removed. Unified rows have two number columns, hunk headers and three
context lines. Click an unchanged fold to expand it; **Fold context** collapses it.
Alt+Up/Down and the hunk buttons navigate changes. Click a text row, Shift-click
another to select a line range, then Ctrl/Cmd+C to copy; **Copy diff / selection**
copies that range or all displayed rows. Selection currently operates on whole
lines. Each file retains its own fold/scroll state while browsing the results.

**Sync selected** runs the active file's chosen action. **Sync all actions** runs
all chosen actions in the comparison, including rows hidden by the text filter.
Both first show the upload/download/delete counts for confirmation. Skip and error
rows are never executed. The runner streams uploads and downloads and executes
all operations serially. Existing regular local and SFTP targets retain their permissions;
adjacent staging files prevent partial content from replacing the old target.
Sources, transfer completion, flush and file close are checked before commit.
The sync report retains confirmed completions, individual errors, cancellation
and unknown remote outcomes, including errors after successful EOF.

Every normally ended sync rebuilds the comparison with its original selection,
mappings and ignore scope, and refreshes both browsers after confirmed changes.
Cancelled or disconnected syncs keep their report and require reconnecting and
comparing again. Transfers are never automatically retried. Hide progress leaves
the operation running; Cancel waits for the active operation's result, closes the
connection and stops subsequent items. Project/window changes cancel in the
background and reject stale completions.

The pinned SFTP library's high-level API lacks the
[OpenSSH POSIX rename extension](https://github.com/openssh/openssh-portable/blob/master/PROTOCOL).
An optional second SFTP subsystem channel on the same authenticated SSH connection
handles that extension. Servers that reject it retain browsing and standard
SFTP rename support. If a server's standard rename cannot replace an existing
target, the upload fails visibly and leaves that target intact; drift never deletes
the target to force a rename or resends a rename after an ambiguous response.

F5 rebuilds the comparison with the same selection and ignore scope. Progress can
be hidden without cancelling. Cancelling a running comparison closes its remote
connection and requires an explicit reconnect. Back from a completed comparison
restores the existing browser session. Project/host changes and connection loss
invalidate comparison handles and reject late results. Connection loss retains
the current sync report while disabling further sync on the stale comparison.

Comparisons use eight or fewer SFTP workers or four FTP connections, a metadata fast path, a 2-MiB text
limit and streaming SHA-256 for larger files. A 60-second idle deadline aborts
stalled comparisons; byte reads and completed scan/compare work reset it.

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
Only one preview reads at a time per session, with a 15-second deadline.

FTP uses native [SuppaFTP](https://docs.rs/suppaftp/12.1.0/suppaftp/) with Tokio.
A session owns at most four control connections; refused extra logins reduce the
pool without retrying transfers. Each connection is reserved until the complete
data stream and final server reply have been checked. Binary transfers preserve
bytes, including CRLF. EPSV falls back to PASV when unsupported; MLST falls back to
SIZE/MDTM and directory listing on servers without MLST. A `550` becomes NotFound
only if a successful ancestor listing proves the missing name; permission errors
remain visible. Keep-alive skips occupied connections. Shutdown interrupts all
owned control and data sockets, including busy transfers. Dropping unfinished
operations invalidates the session rather than reusing an ambiguous control reply.
FTP uploads stage next to the destination and rename only after source close and
successful transfer completion. FTP remote replacement keeps the server's staging
file permissions, matching the Go transport; local downloads preserve target modes.

Explicit FTPS uses Rustls with TLS 1.2 and native certificate roots. Certificate
failures return an asynchronous challenge before any credentials are sent. The
separate prompt shows the endpoint, SHA-256 fingerprint, subject, issuer, names,
validity and verification problems; choose **Reject**, **Trust for this session**
or **Trust permanently**. An exception applies only to that endpoint, fingerprint
and exact problem set. Signatures, key usage and chain constraints remain mandatory.
Permanent entries use the Go-compatible `trusted-certificates.toml` with mode 600,
atomic writes and the shared transaction lock. A changed record keeps the dialog
open with a conflict; failed writes grant no session fallback.

The first connection after approval requires the inspected certificate. Every
additional control connection and protected data handshake is pinned to the
primary certificate, including certificates otherwise signed by a trusted CA.
TLS session resumption is disabled so those checks are never bypassed. A data
certificate change invalidates the session and opens a new prompt. Approval
connects the browser again; interrupted comparisons and transfers require an
explicit new comparison and are never repeated automatically. The small Rustls
stream adapter defers data handshakes until first I/O, so a real preliminary `550`
can be classified without waiting for a data channel the server will not open.

Hosts opens a separate management view and keeps the browser session. Project
hosts and global servers have separate lists; links use an existing global server
and store only their name, server, root path and mappings. Connection forms support
SFTP, FTP and explicit FTPS, password/key file/SSH-agent settings, and optional
keep-alive (empty means 60 seconds, zero disables probes). Passwords/passphrases
are masked. Empty port/user fields keep the stored defaults instead of saving
resolved values. Local/deploy mapping paths remain relative to their roots.

**Test** in the host list or **Test connection** in a form resolves fresh defaults
and server links, checks authentication, root access and directory listing, then
closes its separate connection. Testing a form leaves it unsaved. FTPS uses the
same session trust manager and certificate dialog as the browser, with the first
approved retry pinned to the inspected certificate. Escape/Cancel returns to the
form with its values and focus preserved and stops pending test I/O.

For FTPS targets, **Reset certificate trust** shows the resolved endpoint and its
session/persistent fingerprints. Confirmation removes both exceptions for that
endpoint. Concurrent changes preserve the confirmation and report a conflict;
**Reload trust** obtains a fresh snapshot. Existing connections keep their policy;
future connections verify again. Reset neither reconnects nor repeats transfers.

Save conflicts and validation errors leave the form open. Cancel it, reload the
list and reopen the record to use a newer version. Delete asks for confirmation;
deleting or renaming a server used by project links fails visibly. Closing Hosts
restores browser focus. Management I/O runs on the bounded background pool;
completed changes refresh the current project's resolved configuration without
resetting its file list or preview. **Link host** in the list or **Choose server /
other project** in a form opens a separate picker. It filters names, projects and
hostnames and shows which projects use a server. Selecting another project's host
asks for promotion confirmation before writing anything. Promotion applies the
source project's defaults to the new server, chooses an unused name, and keeps the
source host's root/mappings in its new link. The destination form keeps its own
name, root and mappings, and is saved only through **Save host**.

Promotion writes the global server before updating the source. A changed source
record or changed defaults leave the confirmation open with a conflict;
**Reload targets** loads fresh records. If only the source write fails, the global
copy remains valid and the form displays the partial-completion warning. This
operation is never retried automatically. All reads/checks/writes use the shared
configuration lock; management writes remain outside project directories.

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
connection/listing identities. `comparison.rs` and `comparison/view.rs` own the
comparison lifetime and file list; `diff.rs` owns immutable-data rendering,
direction, folds, source anchors, line selection and scroll state. Shell comparison
entry routing lives separately in `shell/comparison.rs`. Certificate presentation
lives in `certificates.rs`; `shell/certificates.rs` coordinates background trust
writes and identity-checked connection retries. Host forms live in `hosts/form.rs`;
`hosts/tools.rs` owns connection testing, its certificate prompt and trust-reset
confirmation independently of the host list. `hosts/links.rs` owns the cross-project
picker and promotion confirmation; `hosts/linking.rs` adopts the chosen server
without resetting the destination form.

Shared fixtures in `../testdata/parity/` verify Go/Rust mapping and staging policy.
Rust tests use real temporary trees, symlinks, a FIFO, Git processes, transaction
locks and failed completion operations. GPUI tests use real file listings in a
headless test window, including nested navigation, project boundaries, input focus,
failed loads, registration, stale-result rejection and picker/session lifetime.
Two panes in one headless window verify independent filter, selection, focus,
history and cancellation; preview tests verify replacement and clipboard content.
FTP tests start a filesystem-backed local daemon from `../testdata/ftp-server/`
and cover login limits, EPSV/MLST fallback, ambiguous permission failures, busy
keep-alive, completion errors after EOF, staged upload/download/delete, abort and
Go comparison/sync parity. The same application and GUI scenarios also run over real FTPS. Additional TLS
tests cover self-signed certificates, unknown CAs, expired/future certificates, name mismatch, invalid usage
and signatures, exact exceptions, retry/data-channel pins, permanent trust reload
and conflicts. GUI tests exercise rejection, session/permanent trust, a data
certificate change, retained sync outcomes/focus after approval and project
switches while a prompt is open. Temporary Go build caches are released once each
test daemon/probe has been built. The Go parity probe explicitly loads the fixture
CA through the pure Go verifier on Linux/macOS. Capacity-limited TLS tests wait
for observed peer socket cleanup before opening another session.
Host-tool tests cover all three transports, fresh defaults/links, wrong passwords,
root errors, active listing cancellation, unchanged browser connections and
unsaved records. GUI tests cover preserved form focus, certificate decisions,
stale prompts and reset conflicts. Go/Rust process tests exercise trust deletion
and the shared write lock in addition to save/roundtrip behavior.
SSH/SFTP tests start a real Go daemon from `../testdata/sftp-server/` against
isolated temporary trees and use real OpenSSH keys/agents. Go, `ssh-keygen`,
`ssh-agent` and `ssh-add` are required for `cargo test`. Comparison parity tests run
the real Go `app.Load` and `sync.Run` workflows through `../testdata/comparison-probe/` against the
same trees/server and compare file pairs, statuses, suggested actions and binary
classification. Production builds and the
GUI executable are native Rust. Tests include authentication, hashed/pattern/revoked
host keys, key changes, agent and keep-alive timeout, connection loss, scope,
remote navigation and cross-view preview/clipboard. No protocol tests are skipped.
Sync tests compare confirmed actions and final tree contents/permissions against
Go, interrupt active uploads/downloads, stop a real server mid-transfer, and drop
the SSH socket at SFTP CLOSE after EOF. They also cover source/target changes to
symlinks after comparison and servers restricted to one session channel.
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

Milestone 2 is in progress: automatic offers for matching endpoints, project edit/delete
and dashboard/startup restoration, GUI preferences and further host
management controls remain. SFTP transport/browser and comparison/unified diff are
available, including serial upload/download/delete sync. FTP now uses these same
workflows, including FTPS and certificate challenges. Complete CLI and
keyboard/selection parity, packaging and native
release acceptance remain. Blocking local filesystem calls already running cannot
be interrupted by Tokio. Sync waits for their outcomes before reporting completion
or cancellation; browsing discards stale results. Concurrency remains bounded.
