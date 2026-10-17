# GUI test packages

These are **manual test artifacts, not releases**. The Go TUI (`drift`) and Rust
GUI (`drift-gui`) have separate build/package paths. GUI packaging never installs
or replaces the Go binary and never requires Go at runtime.

SSH dependency/host-certificate blockers, native Wayland/X11/macOS acceptance,
WAN/ongoing-I/O performance and the planned Monokai palettes remain open. See
[the port plan](../docs/rust-port-plan.md), [SSH blockers](../docs/rust-ssh-certificate-blockers.md)
and [theme gates](../docs/rust-gui-theme-blockers.md). Packaging does not approve
integration, public distribution or transfers.

## Native builds and identifiers

Supported package targets:

| Target | Manual CI runner | Artifact |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Ubuntu 24.04 | tar.gz with binary and Desktop entry |
| `x86_64-apple-darwin` | macOS 15 Intel | tar.gz with `Drift GUI.app` |
| `aarch64-apple-darwin` | macOS 15 Apple Silicon | tar.gz with `Drift GUI.app` |

No universal binary, cross-architecture execution or static/portable Linux
runtime is implied. Build on the target architecture. macOS deployment minimum
is 15.0; Intel and ARM require separate native acceptance. Linux artifacts built
locally inherit that machine's ABI/runtime, not the CI runner's compatibility.
Native GUI rendering still needs a usable display/GPU/backend and system fonts.
Repository discovery, Finder and gitignore classification use the `git` executable
on PATH. Supply Git separately (on macOS, for example via Command Line Tools or
a package manager); it is not bundled. Go/SSH command-line programs are not the
transport runtime.

`DRIFT_GUI_VERSION` is a **build-time** SemVer identifier without a `v` prefix:

```sh
(cd rust && DRIFT_GUI_VERSION=0.1.0-dev.1 cargo build \
  --locked --release -p drift-gui)
rust/target/release/drift-gui version
```

Unset uses the Cargo package version. Empty/invalid values fail the build without
printing the supplied value. Identifiers are at most 128 ASCII bytes; each core
component is at most 65535. Prerelease/build metadata remains exact in the CLI,
startup log and artifact filename. Changing this environment variable at runtime
cannot change an already built binary. It is independent of Go's `VERSION` and
`release-build` target. The build reuses the already locked SemVer crate version;
no SDK fork or new dependency package version is introduced.

## Assemble locally

Python 3.13+ with `waitid`/`WNOWAIT` and the pinned Rust toolchain are needed for
packaging, not for using
the extracted GUI binary. Run from the repository root:

```sh
make rust-package-test
make rust-package GUI_VERSION=0.1.0-dev.1
```

The native Rust host target is the default. `GUI_TARGET`,
`GUI_CARGO_TARGET_DIR` (respects `CARGO_TARGET_DIR`) and `GUI_DIST_DIR` can override
build/output locations. On macOS, also set `MACOSX_DEPLOYMENT_TARGET=15.0`.
Output defaults to ignored `dist/gui/`. Existing final archive/checksum names are
not overwritten; choose a fresh version or output directory.

For an already built native binary:

```sh
python3 rust/packaging/package.py \
  --binary rust/target/release/drift-gui --version 0.1.0-dev.1 \
  --target x86_64-unknown-linux-gnu --output dist/gui
```

The assembler checks binary type, architecture and exact embedded version with
an isolated display-free invocation before assembling. Use only trusted local
builds: invoking a binary is not a security sandbox. Files are staged; final
archives include a SHA-256 sidecar. Checksums establish integrity, not publisher
identity or signing. Archive metadata is normalized and `SOURCE_DATE_EPOCH`
controls timestamps; this does not claim reproducible Cargo binaries.

## Linux layout and manual installation

The extracted directory contains `bin/drift-gui`, a freedesktop Desktop entry,
license and runtime/test-gate notes. No shared libraries, Rust, Go or Python are
bundled. Install the runtime dependencies required by that native build; a newer
local glibc cannot be assumed compatible with Ubuntu 24.04. Inspect ELF dependencies
without executing `ldd` on untrusted files, for example using `readelf -d` and
`readelf --version-info`. Optional rendering drivers/backend libraries can be
loaded dynamically and may not appear in that inventory.

After checking the artifact and choosing to run the test build, install manually
from its extracted directory:

```sh
install -Dm755 bin/drift-gui "$HOME/.local/bin/drift-gui"
install -Dm644 share/applications/io.github.WariKoda.drift-gui.desktop \
  "$HOME/.local/share/applications/io.github.WariKoda.drift-gui.desktop"
```

Both terminal and desktop-session PATH must include `~/.local/bin`; the launcher
uses `Exec=drift-gui` and `TryExec=drift-gui`. This separate name does not replace
`drift`. There is no file/URL association: desktop OpenFile/OpenURL event routing
is not implemented. Uninstall only these two installed files; shared project/
host/trust stores and `gui.toml` are deliberately retained.

## macOS bundles

The archive contains `Drift GUI.app/Contents/MacOS/drift-gui`, an XML Info.plist
and license. The identifier is `io.github.WariKoda.drift-gui`; minimum macOS is
15.0. CLI SemVer remains exact in `DriftBuildVersion`, while the standard bundle
version fields use its numeric core. These are test build identifiers, not an
App Store build-number policy or a release upgrade mechanism.

Bundles are **not Developer-ID signed or notarized**. The native linker/toolchain
may already ad-hoc sign Mach-O code; this is not publisher signing or notarization.
Gatekeeper may refuse a downloaded bundle. No quarantine bypass, blanket security
exception, installer or signing credentials are supplied. Use controlled native
test machines and the platform's security policy; regular distribution must add
reviewed signing/notarization later. macOS launch/rendering, relocation and
Gatekeeper behavior have not been established by Linux archive/plist tests.

After native checks, copying the app to a user-selected Applications directory is
manual. The bundle executable remains usable for CLI management/version commands.
No files, URLs or remote connections are automatically opened by associations.

## Manual CI, no publishing

`.github/workflows/gui-artifacts.yml` has **only `workflow_dispatch`**, with a GUI
version input and read-only repository permissions. It builds all three native
targets without Go, runs structural tests, validates/assembles packages and
uploads short-lived Actions artifacts. It does not create tags, GitHub Releases,
package-registry uploads or credentials. Artifact visibility follows repository
permissions; do not assume Actions artifacts are private on a public repository.
Regular CI runs packaging unit/process tests; that alone is not native bundle acceptance.

GitHub can dispatch a workflow only once its definition exists on the default
branch. While this is an unmerged stacked draft, it is preparation, not a claim
that the manual matrix already ran. After review/integration and selecting the
intended source ref:

```sh
gh workflow run gui-artifacts.yml --ref <reviewed-ref> -f version=0.1.0-dev.1
```

Verify workflow source/ref, all native results and artifact contents. The Go CI
and release commands stay independent. Turning these artifacts into public GUI
releases requires a separate explicit decision after the open gates are resolved.

References: [GitHub runner architectures](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[Apple bundle build version](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleversion),
[Apple short version](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleshortversionstring).
