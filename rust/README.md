# drift-gui

The Rust desktop application develops alongside the Go TUI. This branch
implements the foundation of milestone 1; it is **not yet a file browser or a
sync client**.
The window displays 10,000 generated filenames to exercise retained input state,
filtering, selection, virtualization, focus actions and native clipboard access.
It does not load or modify the user's drift configuration.

## Standalone applications

The Go TUI and Rust GUI remain independently maintained products in one
repository. Each has its own executable, build dependencies, installation,
version and release cycle. Neither application invokes or requires the other
at runtime. The TUI currently installs as `drift`; the GUI installs as
`drift-gui`. Existing TUI commands and installation paths remain available.

The Rust core is implemented in Rust rather than wrapping the Go application.
Shared behavior is specified through parity fixtures and tests, so changes to
one implementation can be checked against the other without a runtime bridge.

Once the persistence milestone is complete, both applications use the same
configuration directory, registry, project hosts, mappings and certificate
exceptions. UI preferences stay separate: terminal preferences in the existing
global config, GUI preferences in `gui.toml`. Shared store changes must preserve
the documented format for both applications and use the complete transaction
lock described below. Independent product versions do not permit uncoordinated
changes to shared file formats.

## Build and run

Rust is pinned in `rust-toolchain.toml`; with rustup, entering this directory
selects the compiler and installs the specified rustfmt/Clippy components.
`Cargo.lock` fixes GPUI Kit and its matching GPUI snapshot together. Do not
upgrade individual `gpui-pre-*` packages independently.

From the repository root:

```sh
make rust-check
make rust-test
make rust-build
make rust-run
# Optional: install drift-gui into Cargo's binary directory.
make rust-install
```

`make rust-build` creates `rust/target/release/drift-gui`. Existing Go build and
installation targets continue to build the TUI.

Linux needs a graphical Wayland or X11 session and a working Vulkan driver.
For Ubuntu 24.04, install the build dependencies:

```sh
sudo apt-get install gcc g++ clang pkg-config libfontconfig-dev libwayland-dev \
  libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev libvulkan1
```

macOS requires macOS 15+ and Xcode Command Line Tools. For local release builds,
set `MACOSX_DEPLOYMENT_TARGET=15.0`, as CI does. Platform requirements follow the
[GPUI Kit installation guide](https://gpui-kit.com/docs/installation/).

## Crate boundaries

- `drift-core`: UI-independent mapping validation, path translation and staging
  name classification. Translation is lexical policy, **not** filesystem
  confinement; it must not be used to authorize local file operations.
- `drift-app`: filtered file-list state with stable selection identities. No GPUI
  dependency; application workflows and resource ownership will live here.
- `drift-gui`: native window, Kit input/button/theme, contextual actions and
  virtualized rendering. Views perform no filesystem or network operations.

The shared `../testdata/parity/*.toml` fixtures are consumed by Rust integration
and Go package tests. They establish mapping fallback, host precedence, mapping
scope, segment boundaries, ambiguous overlaps and staging exclusions against the
existing Go implementation. They do not establish protocol or persistence parity.

The GPUI interaction test runs in a headless test window and checks typing,
retained focus, filtered row selection, copying and offscreen row omission. It
cannot establish native rendering or real OS clipboard integration. CI builds
and tests on Ubuntu 24.04 and macOS 15; native platform acceptance requires the
manual checks below.

## Platform acceptance checks

Run `make rust-run` separately in Linux Wayland, Linux X11, macOS Intel and macOS
Apple Silicon sessions, and record OS, renderer and result with the PR:

1. Resize the window and scroll the 10,000-row list from beginning to end.
2. Enter `00042` in the filter; only `file-00042.txt` should remain.
3. Click that row, then Copy filename. Paste into another application and verify
   the exact filename. Repeat after selecting another row.
4. Use Ctrl+F (Cmd+F on macOS) to focus the filter; typing must edit the input.
5. Add `x` to the filter; the list becomes empty and Copy filename is disabled.
6. Close the window; the process must exit.

## Remaining milestone work

Milestone 1 still requires native rendering/input/clipboard verification on all
supported platforms. Later milestones are not implemented on this branch:

2. Shared TOML/registry persistence, complete Go/Rust write transactions and
   conflict detection, capability-confined local browser, finder and preview.
3. SFTP authentication/connection lifecycle, comparison, unified diff and sync.
4. FTP/FTPS, trust prompts, keep-alive and real-server protocol tests.
5. All management and keyboard workflows and Rust CLI parity.
6. Linux packages, dual-architecture macOS bundles and release acceptance.

Do not allow Rust writes to shared stores before both applications implement
`<config.Dir()>/write.lock` around the complete read/validate/update/write
operation. Go currently has no such shared transaction lock. Do not advertise
sync support or release parity until real-server and cross-process tests pass.
