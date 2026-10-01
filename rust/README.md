# drift-gui

The Rust desktop application develops alongside the Go TUI. It currently provides
a local browser with directory navigation, filtering, a project-wide finder and a
read-only UTF-8 text preview (up to 1 MiB). The Projects panel opens registered
projects or registers the current folder. Remote browsing and sync are not yet
implemented.

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
its capability root and hosts; the longest registered path wins. Up cannot leave
that project root. Click a directory to enter it or a file to preview it in the
opposite pane. The finder searches the entire project; the filter narrows the
returned paths. Hidden and ignored entries have separate visibility toggles.
Fixed exclusions and interrupted transfer staging files remain excluded.

Preview uses Kit's read-only text control with line numbers, wrapping and native
text selection/copy. Copy text copies the complete preview; Copy path copies the
selected project-relative path. Ctrl+F/Cmd+F focuses the file filter, F5 refreshes,
and Cancel stops the active listing/finder/preview. Closing Projects keeps the
browser session. Directory and preview responses carry separate generations;
stale responses are discarded and their root handles released.

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

Shared fixtures in `../testdata/parity/` verify Go/Rust mapping and staging policy.
Rust tests use real temporary trees, symlinks, a FIFO, Git processes, transaction
locks and failed completion operations. GPUI tests use real file listings in a
headless test window, including stale-result rejection and picker/session lifetime.

Cross-process Go/Rust store tests run in Linux and macOS CI. To run them locally:

```sh
cd rust && cargo build --locked -p drift-core --example store_probe
cd ..
DRIFT_RUST_STORE_PROBE="$PWD/rust/target/debug/examples/store_probe" go test ./internal/parity
```

All test stores use temporary directories. Native Wayland, X11, macOS Intel and
Apple Silicon rendering/OS clipboard checks remain manual: navigate a temporary
project, filter and find files, preview/copy text into another app, toggle hidden
and ignored paths, change projects during loading, close the picker, and close the
window. Headless tests cannot establish native rendering or OS clipboard behavior.

## Remaining port work

Milestone 2 is in progress: full host forms/server promotion, project edit/delete
and dashboard/startup restoration, GUI preferences and full certificate-store
roundtrip coverage remain. Later milestones add SFTP, FTP/FTPS, certificate
challenges, keep-alive, comparisons/unified diff, serial sync, complete CLI and
keyboard parity, packaging and native release acceptance. Safe atomic-write
primitives are tested; there is no transfer implementation yet. Blocking local
filesystem calls already running cannot be interrupted by Tokio; their eventual
results are discarded after cancellation and concurrency remains bounded.
