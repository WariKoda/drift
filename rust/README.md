# drift-gui

See the [Rust/GPUI port plan](../docs/rust-port-plan.md) for milestones and the
planned System/Dark/Light modes with Monokai Pro Dark and Monokai Pro Light Sun.

The Rust desktop application develops alongside the Go TUI. It currently provides
a local browser with directory navigation, filtering, a project-wide finder and a
read-only UTF-8 text preview (up to 1 MiB). The Projects panel opens registered
projects or registers the current repository/folder. Its dashboard creates, edits,
archives/unarchives and removes projects after confirmation. Hosts manages project targets and global
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

Project management commands run before GUI initialization and need no display:

```sh
drift-gui projects list
drift-gui projects add "Shop" /path/to/shop
drift-gui projects edit shop --name "New Shop" --path /new/path
drift-gui projects archive shop   # toggles archived/active
drift-gui projects remove shop    # removes registry/settings; keeps local files
drift-gui open shop               # starts the GUI for a matched project
drift-gui dash                    # starts on the dashboard
drift-gui version
```

Add defaults to the current directory. Add/edit paths support `~` and `~/`.
Open matches an exact slug, exact name, unique prefix or unique substring, including
archived projects. Invalid or ambiguous matches fail before opening a window;
the opening timestamp is written only after the GUI loads the project. Listing
includes active, archived and missing paths. Mutations use the shared write lock
and preserve project identity, timestamps and settings where applicable.
`projects --help` documents options; help/version need no configuration directory.
Version currently reports the Cargo package version; release packaging remains open.
Use `--` before a directory named `projects`, `open`, `dash` or `version`, or one
beginning with `-` (`drift-gui -- projects`); `./projects` also selects that folder.
Project commands accept `--` before positional arguments with leading hyphens.
Invalid syntax, validation failures and lock contention return exit code 1.
Optional file logging is available for both GUI starts and project commands.

## Optional diagnostics

Logging is off by default and opens no file unless enabled:

```sh
drift-gui --debug                         # <config dir>/drift.log, debug level
drift-gui --log /tmp/drift-gui.log /path/to/project
drift-gui projects list --log /tmp/drift-gui.log
DRIFT_LOG=/tmp/drift-gui.log DRIFT_DEBUG=1 drift-gui open shop
```

A nonempty `--log` path overrides `DRIFT_LOG`; an empty flag falls back to the
variable. `--debug` or a truthy `DRIFT_DEBUG` enables debug records; without a path,
logging uses `<config.Dir()>/drift.log`. Go's boolean rules apply: empty, `0`, `f`,
`F`, `false`, `FALSE` and `False` are off; other nonempty environment values are on.
`--debug=false` does not veto a true environment value. Logging flags work before
or after commands and arguments; `--` ends flag parsing. Help/version ignore
logging configuration and create neither configuration directories nor log files.

Records append to the selected file; new files have mode 600. A background writer
keeps log I/O off the GUI thread. After the window loop ends, tracked background
operations finish cancellation/transport cleanup before logs drain and close. Open failures
warn and continue without logging. Write/close failures disable logging and remain
visible in a persistent GUI banner and/or a CLI stderr warning; they do not change
committed mutations, typed errors or sync reports. Connection, comparison and sync
records include operation identities, host/endpoint, paths, stages and outcome
counts. They omit authentication fields and file contents. Error categories and OS
error codes are recorded instead of raw server replies or TOML decode messages,
which could expose secrets. Diagnostics may contain hostnames and file paths.

## Browser and management

The GUI stays in the current directory inside registered projects and unregistered
Git repositories. Outside them, it restores the last opened active project if its
path is usable; otherwise it shows the dashboard when projects exist. A directory
argument opens that directory directly. `--dashboard` forces the project list;
`--no-dashboard` takes precedence and stays in the chosen/current directory.
`--help` shows these options without opening a window. A registered containing project supplies
its capability root and hosts; the longest registered path wins. Up stays within
registered projects. For unregistered folders, Up opens the parent with a new
capability root. Open folder uses the native folder chooser. Back and Forward
follow successfully loaded directories; a failed load keeps the current folder.
Each browser displays an expandable file tree. Click a directory's arrow to
load its children in place; click it again to collapse them. A collapsed directory
shows how many marked descendants it contains. Children load through the existing
background services, and failed loads leave the directory collapsed. Refresh
restores expanded paths and the cursor. Visibility changes retain expansion state,
including directories that are temporarily hidden.
Click a directory's name to enter it or a file to preview it in the opposite pane.
Ctrl-click/Cmd-click toggles a mark without opening the entry; Shift-click adds a
visible range. Marks have a checkmark separate from the cursor highlight and a
count in each pane. They survive navigation, filters, visibility changes and
refresh, and clear when the project or remote connection changes.
The finder searches the entire project; the filter narrows the
returned paths. Hidden and ignored entries have separate visibility toggles.
Fixed exclusions and interrupted transfer staging files remain excluded.

Right-click a file or folder to move only the cursor and open its context menu;
marks, ranges, filters and the loaded preview stay unchanged. The menu offers
preview/folder navigation, tree expansion, marking, copying paths, comparison and
browser controls. Right-click empty list space for pane-wide controls without
acting on the old cursor. **Compare this file/folder** compares only that entry,
regardless of marks; **Compare marked files** combines marks from both panes,
including collapsed descendants. Mapping and exclusion rules still apply.
Shift+F10 or the Menu key opens the same menu for the focused browser's cursor.
Arrow keys or Tab/Shift+Tab navigate items; Enter selects one. Escape dismisses
without clearing marks or filters
and returns focus to that pane. Outside clicks dismiss and still reach the clicked
control. Browser/global shortcuts do not run behind an open menu. Loading disables
file/comparison actions; Cancel loading and remote Disconnect remain available.
Context menus and comparisons never start transfers: the existing sync
confirmation is still required.

Drag the divider to resize browser/preview/remote or comparison-list/diff panes.
Ctrl+Alt+Left/Right moves it in 24-pixel steps; Ctrl+Alt+0 restores the default
split (half-and-half in the browser, a 350-pixel comparison list where space
allows). These shortcuts also work from filters and previews but not behind
context menus. Dragging preserves keyboard focus; Escape stops the resize
without clearing filters/marks or cancelling file work. Minimum widths adapt to
small windows. Relative sizes survive screen/project switches and window resizing
within the session; they do not start previews, comparisons or transfers. The
existing child entities keep navigation, selection, folding and scroll state.
Control rows wrap when a pane becomes narrow. Pane/window persistence and GUI
preferences remain open and will use `gui.toml`, not shared Go configuration.

**Projects** opens the dashboard without replacing the browser or remote session.
Closing it or choosing the already active project preserves navigation, selection
and connection. The list filters names, slugs and paths and can show archived
projects. **New project** accepts a name and local path (including relative paths
and `~/`); **Register current folder** suggests the containing Git root, including
worktrees. Edits keep the slug, creation/open timestamps, hosts and mappings.
Archive/unarchive changes visibility and the edit timestamp, retaining settings.
A changed record produces a visible conflict and retains the form/confirmation;
cancel and reload to use the current snapshot.

In Projects, Down/Enter leaves the filter and focuses the list. Arrows/J/K and
Home/g or End/G navigate the visible projects. Enter opens the selected project;
1–9 directly open the corresponding visible row, with numbering updated by filters
and archived visibility. Digits remain text in inputs. The n key creates, e edits,
a archives/unarchives, d/Delete asks to remove, a dot shows/hides archived projects,
and r reloads. The / key or Ctrl/Cmd+F returns to filtering. Ctrl/Cmd+S saves a form;
Enter at the default confirmation target or y confirms removal; Escape cancels.
Focused buttons keep native Enter/Space behavior, including Cancel. The cursor follows the project slug
across edits and reloads, and scrolls into view. Filtered-out projects cannot be
acted on through list commands.

Removal first hides the project's host store, commits registry removal under the
shared lock, then deletes the hidden settings. A failed registry commit restores
the settings; a cleanup failure after commit is reported explicitly. Local project
files are preserved. Moving/removing the active project cancels its operations,
closes its remote session and invalidates the old capability root. The browser
reloads against the moved path or the remaining registry; reload failures stay
visible. No management operation writes into the local project tree.

Preview uses Kit's read-only text control with line numbers, wrapping and native
text selection/copy. Copy text copies the complete preview (including an empty
loaded file); Copy path copies the selected project-relative path. In a browser,
p toggles the selected file preview and c copies its loaded text. Ctrl/Cmd+Alt+P
focuses the read-only editor for native selection, copy and scrolling; Escape
closes that preview and restores its originating browser without disconnecting.
Ctrl+F/Cmd+F focuses the file filter; Down/Enter/Escape returns to results without
opening a file or clearing the query. F5 refreshes. Ctrl+Escape or Cancel stops
active listing/finder/preview work, also while the filter has focus. In the browser, arrows or
J/K move the cursor; Home/g and End/G select the first/last visible row.
Tab/Shift+Tab switches between the local and remote browsers, opening the remote
pane when necessary. P opens Projects, H opens Hosts, and @ opens Remote.
The / key focuses the active filter; f opens the local project finder and focuses
its initially empty query. Finder uses case-insensitive Unicode-lowercased subsequence
matching across project-relative paths, preferring adjacent and word/path-boundary
matches; the ordinary browser filter remains a substring filter. This is not full
Unicode normalization/case folding or byte-for-byte Go ranking. Repeated f only
focuses the existing Finder query. Return to browser or Ctrl/Cmd+Alt+F restores the
saved tree, cursor, filter, range and file-list scroll while keeping marks changed
in Finder, including files in collapsed folders. Alt+Left also returns from Finder;
refresh/navigation first restores the saved browser before performing its normal
operation. Finder entry is ignored while an ordinary directory listing is busy.
A dot toggles hidden files in the active pane, I toggles ignored files there,
and r refreshes the active browser. Remote Show ignored uses the current project's
Git policy after host/project path mapping, including remote-only paths. It is a
cached visibility toggle, not a transfer or comparison-scope option. Hidden ignored
marks survive; unmapped paths remain browsable but unmarkable. Raw and mapped hard
exclusions remain excluded even when Show ignored is on. Classification runs in
background work; loading disables marking/comparison and remote visibility toggles.
Git/mapping errors remain visible without closing the connection; Refresh reclassifies.
Cancelling a replaced local classification does not disconnect or retry remote work. Enter/Right/L expands a directory, or moves to its first
visible child when already expanded; on files it opens the preview. Left/H collapses
the directory or its parent, preserving descendant marks. Alt+Enter enters the
selected directory as the new browser root. Backspace/Alt+Up goes up, and
Alt+Left/Right follows history. Space toggles the cursor's
mark; V marks visible siblings in its directory; * inverts visible marks.
V in the finder uses the cursor's parent directory. Lowercase v starts/finishes
an additive visible interval; Shift+Up/Down adds a range immediately. Escape
cancels an interval first, then clears a filter, then clears the pane's marks.
Marking preserves the loaded preview. These browser shortcuts leave
text inputs free for typing. Projects has a name/slug filter and registers the
displayed folder. Closing Projects keeps the browser session and restores focus.
F1 opens scrollable shortcut help from browsers, forms and dialogs; ? opens it
from browser/diff lists. Escape closes help and restores its invoking focus.
Underlying operations continue, but commands do not run behind help or menus.
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
pane's marked paths, falling back to the cursor's file or current folder when
nothing is marked. Press s in either browser to compare marks from both panes
without falling back to the whole project. Remote paths outside the effective
mappings remain browsable but cannot be marked. Directories expand
both counterparts recursively, including files that exist on only one side.
Hidden visibility does not narrow comparison scope. **Include ignored** is a
separate operation setting; directly selected ignored files remain exceptions,
while fixed exclusions and transfer staging files never enter the comparison.

The comparison shows differing files and per-file errors, with suggested actions.
Click the action button or press Enter/Space in the file list to cycle valid
previews. Home/g and End/G jump to the first/last visible file; n/p select the
next/previous file from either the list or the diff. Tab/Shift+Tab switches focus
between those two areas. The r key refreshes and i toggles ignored paths.
Upload shows Remote → Local, Download Local → Remote and deletion shows
the affected side being removed. Unified rows have two number columns, hunk headers and three
context lines. Click an unchanged fold to expand it; **Fold context** collapses it.
Alt+Up/Down, [/] and the hunk buttons navigate changes. In the diff, Up/Down/J/K
scroll by line, PageUp/PageDown by page, Ctrl+U/D by half a page, and Home/g or
End/G to the start/end. Enter/l expands the first visible folded gap, h collapses
a visible expanded gap, and c toggles all foldable gaps. Space cycles the current
action; A cycles every valid action, including files hidden by the filter.
Click text to place a caret; drag or Shift-click to select characters across lines.
Selection follows extended graphemes, so combining characters and joined emoji
are not split. Click the number/sign gutter to select a whole content line.
Left/Right moves the caret; Shift+arrows extends the selection, Shift+Home/End
extends to the line edge, and Ctrl/Cmd+A selects all visible content.
Ctrl/Cmd+C or **Copy diff / selection** copies selected content only: no numbers,
+/- prefixes, hunk headers, fold placeholders or hidden context. Whitespace and
blank lines are retained. Without a nonempty selection, copy still produces the
formatted displayed diff. Text is the comparison's decoded, newline-normalized
content, not an original-file byte export.

Dragging near the viewport edge scrolls through virtualized rows; release or
Escape ends that gesture without clearing its range. Pane resize keeps priority.
Each file retains its own selection/fold/scroll state while browsing results.
Refresh retains it for unchanged comparison content identified by the exact
local/remote path pair; changed, removed or failed comparisons discard stale
coordinates. Fresh sync suggestions and explicit confirmation are unaffected.

**Sync selected** runs the active file's chosen action. **Sync all actions** runs
all chosen actions in the comparison, including rows hidden by the text filter.
The s/S keys prepare selected/all actions from the file list or diff.
The u/d keys choose Upload/Download for the current file and open that same
confirmation; a missing source or a file error leaves the action unchanged.
The confirmation shows the upload/download/delete counts; Ctrl/Cmd+Enter
confirms from list, diff, filter or button focus; Escape in list/diff dismisses
the pending confirmation. Escape in a filter only returns to results. Shortcuts never execute
a transfer without that confirmation. Skip and error
rows are never executed. The runner streams uploads and downloads and executes
all operations serially. Existing regular local and SFTP targets retain their permissions;
adjacent staging files prevent partial content from replacing the old target.
Sources, transfer completion, flush and file close are checked before commit.
The sync report retains confirmed completions, individual errors, cancellation
and unknown remote outcomes, including errors after successful EOF.
Press e in the comparison list or diff (or click **Show errors**) to toggle
scrollable Failed/Unknown details. Escape closes details before leaving the
comparison. The summary and error count remain visible; refresh and connection
loss preserve the report, while a new sync clears it.

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
or **Trust permanently**. Reject is initially selected. Tab/Shift+Tab or
Right/L and Left/H change the choice; Enter confirms and Escape rejects.
Arrows/J/K, PageUp/PageDown and Home/End scroll certificate details while the
decision buttons remain visible. Pending approval disables further trust decisions. An exception applies only to that endpoint, fingerprint
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

Hosts also uses Down/Enter from the filter, arrows/J/K, Home/g and End/G,
and / or Ctrl/Cmd+F for searching. In the list, n creates, e/Enter edits,
c duplicates, d/Delete asks to delete, t tests the connection, r opens FTPS trust
reset, l opens the link picker, and F5 reloads. Tab/Shift+Tab switches between
project hosts and global servers when a project is open. Ctrl/Cmd+S saves the
current form; Enter at the default confirmation target or y confirms deletion,
and Escape returns to the list with its cursor retained. Enter/Space activates
focused native buttons; Cancel/Back/Reload never turns into confirmation. Without
a project, list Tab/Shift+Tab follows normal control traversal instead of doing
nothing. The inactive list leaves Tab order while editing. Host fields, mapping
inputs and native controls reveal themselves within the actual external scroll
viewport on Tab/Shift+Tab or viewport resize. Ordinary typing/redraws and wheel
scrolling do not snap the view back. Mappings and actions wrap; removing a mapping
focuses the related remaining local input, or Root path after the last removal.
Draft/input entities and masked secrets remain unchanged. Letters in filters and
form fields remain ordinary text; search shortcuts do not move focus out of an active form. Tools, link-picker auxiliary
controls/confirmations and project edit/delete details have bounded scroll viewports
and wrapping controls. The link/project list keeps its own scroll/cursor identity;
native row buttons reveal within that list. New deletion/promotion confirmations
start at the identifying details without resetting list scroll. Native Tools focus
falls back temporarily during async work and is restored only to a still mounted,
enabled control in the completed new frame. Enter on that temporary owner is not
implicit trust approval; explicit `y` and affirmative controls remain available.
Validation/conflict errors keep
the form or confirmation open.

**Test** in the host list or **Test connection** in a form resolves fresh defaults
and server links, checks authentication, root access and directory listing, then
closes its separate connection. Testing a form leaves it unsaved. FTPS uses the
same session trust manager and certificate dialog as the browser, with the first
approved retry pinned to the inspected certificate. Escape/Cancel returns to the
form with its values and focus preserved and stops pending test I/O.

For FTPS targets, **Reset certificate trust** shows the resolved endpoint and its
session/persistent fingerprints. Confirmation removes both exceptions for that
endpoint. Concurrent changes preserve the confirmation and report a conflict;
**Reload trust** (r) obtains a fresh snapshot. Enter at the default target or y
confirms the reset; focused Return/Reload/Test buttons retain native Enter/Space.
Escape returns to Hosts. Reject remains usable while approval is being saved and
prevents the test retry; cancelling does not undo already committed trust, and
the status explicitly warns to reload/reset it before retrying. Existing connections keep their policy;
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

In the picker, Down/Enter leaves the filter; arrows/J/K and Home/g or End/G move
the visible cursor. Enter selects a global server or opens promotion review for
a project host. Enter at the default confirmation target or y confirms promotion;
Enter/Space on Back/Reload keeps its labelled action. Escape returns to the same target.
The / key or Ctrl/Cmd+F searches, and r reloads targets after a conflict. Filter
text remains editable. The cursor is identified by project and host name and
survives reloads; failed global selection also restores usable dialog focus.

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
`projects.rs` owns the dashboard, filtering and confirmation;
`projects/form.rs` owns form inputs, `projects/management.rs` adopts typed service
results, and `shell/projects.rs` routes startup and active-root changes. `toolbar.rs` renders stateless controls
and `actions.rs` defines key bindings. `remote.rs` owns the remote pane and its
connection/listing identities. `comparison.rs` and `comparison/view.rs` own the
comparison lifetime and file list; `diff.rs` owns immutable-data rendering,
direction, folds, source anchors, grapheme selection and scroll state. Its `diff/`
modules use native shaped-text geometry and an application-owned logical range
rather than pixel endpoints or independent virtual-row selection participants. Shell comparison
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
Keyboard tests cover list boundaries, pane switches, management entry/return,
typing command letters into filters, and confirmed/cancelled sync over real SFTP.
Tree tests cover delayed local/SFTP loading, nested collapse, hidden and ignored
visibility, remembered expansion, vanished directories, discarded completions,
project switches and comparison/sync scope with marked collapsed children. The
FTP/FTPS shell tests also expand, mark and collapse real remote directories.
Mapped remote visibility tests cover remote-only/negated/tracked ignored paths,
host-over-project mappings, off-tree directory marks, hard-excluded restoration,
external root prefixes, stale classifier results, cancellation and real Git errors.
Cached native button/menu/keyboard toggles preserve the session and issue no FTP
commands; input/help/popup guards prevent letter commands and implicit transfers.
Finder failure fixtures corrupt a real Git index after opening the browser, so
failure restoration and focus guards also run on macOS filesystems.
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
Project tests verify real deletion rollback, archive/settings preservation, startup
overrides/restoration, Git-root suggestions, stale forms and active-root invalidation
with a live FTP connection. Go/Rust process tests cover archive/removal, shared locks
and competing Go edits before Rust save/delete.
Host tests use real stores to verify forms, CRUD, duplication, defaults, links,
masked fields, concurrent-edit conflicts and deletion guards. Management keyboard
tests cover empty/filtered lists, scrolling, cursor retention, archive/unarchive,
scope switching, form validation, saved-host connection tests and trust-reset
conflicts. Host-form scroll tests assert actual visible control bounds in short,
narrow and inset windows, forward/reverse native Tab, additions/removals and
protocol/auth/link changes. They preserve draft entities/masking, test resize and
stale frames, hidden async completion and wheel scrolling without snap-back.
Fifteen additional Tools/Links/Projects regressions use real stores/FTPS to check
bounded details/actions, native labelled controls, preserved list/input identities,
confirmation reopening, resize/wheel behavior and cancellation/certificate guards.
Focus tests reject removed/disabled targets and prevent temporary container Enter
from resetting trust before deferred restoration; native Reload Enter remains Reload.
Input-boundary tests exercise actual clipboard actions and the production native
InputHandler adapter, including raw controls before normalization, UTF-16 ranges,
Unicode, selection/composition/undo, readonly/stale inputs, accessibility metadata,
native Tab/Shift-Tab and rejected IME cleanup across adapter repaint. Real stores
verify blocked save/test/registration and rejected stored values without rewriting
secrets or paths. These tests do not establish OS IME/accessibility integration or
permission-gated clipboard completion on native platforms.
Numeric shortcut tests cover all nine rows, filtering, archive visibility
and text input. Shell tests open projects through keyboard events and the CLI start result.
CLI tests execute the actual binary without display access to check help, version,
project mutations, error exits and a lock held by another process. Logging tests
check flag/environment priority, disabled/help/version paths, private append files,
concurrent drain, open/write failures, retained GUI warnings across dialogs and
comparisons, redaction and real SFTP/FTP/FTPS connection/comparison/sync outcomes.
Closing the last headless window during a real upload verifies terminal records
survive cancellation and transport shutdown. Picker tests also
exercise keyboard promotion, filtering, reloads and partial writes. Certificate
tests use real FTPS challenges to check initial rejection, pending-approval guards
and detail scrolling. Diff tests check page boundaries, folds, source selection,
bulk action cycling and confirmed direct transfers over SFTP.
Error detail tests use real SFTP failures, active cancellation and server loss
to check focus and prevent stale transfer retries, and real FTPS reconnects to
check retained certificate-related failures. Resize tests retain browser navigation,
marks/ranges, filters, preview IDs, diff decisions/folds/selection and scroll state;
an actual SFTP upload completes without cancellation or an extra transfer. They
exercise minimum sizes, inset/tiny containers, unclamped preferences on shrink/grow,
menu dismissal, keyboard control and implicit Kit Button focus, real input composition,
no-frame cancellation and stale gesture callbacks across screen/project changes.

Cross-process Go/Rust store tests run in Linux and macOS CI. To run them locally:

```sh
cd rust && cargo build --locked -p drift-core --example store_probe
cd ..
DRIFT_RUST_STORE_PROBE="$PWD/rust/target/debug/examples/store_probe" go test ./internal/parity
```

All test stores use temporary directories. Native Wayland, X11, macOS Intel and
Apple Silicon rendering/OS clipboard checks remain manual: navigate a temporary
project, use the native folder chooser, navigate back and forward, filter and find
files, mark files/folders with Space, Ctrl/Cmd-click, Shift-click, v/V and *,
expand/collapse nested directories, refresh and verify cursor/expansion restoration,
change filters/visibility/folders and confirm both panes retain their own marks,
compare with s and confirm refresh/sync keep only that scope, exercise host forms
and return to the browser, preview/copy text into another app, toggle hidden
and ignored paths, change projects during loading, drag and keyboard-resize both
browser and comparison panes while loading/syncing, test Escape and menus during
resizing, shrink/grow a client-decorated window, use preview focus/copy and F1 help,
activate Cancel/Back through native Tab/Enter, close the picker, and close the window. Headless tests cannot establish native rendering or OS clipboard behavior.

## Single-line input boundaries

Form fields and filters reject the **entire insertion** if it contains C0/C1
controls (including Tab, NUL, CR/LF and DEL) or U+2028/U+2029 line separators.
Clipboard, native text/IME and exposed accessibility entry are checked before
native single-line normalization. Existing text, selection, composition and undo
remain unchanged on rejection; warnings never echo the rejected contents.
Unicode and spaces pass through unchanged; ordinary field-specific validation
still applies. Passwords and paths are not silently cleaned.

With native composition marked, Escape or Enter ends only that composition,
keeping its current preedit text as the native SDK does. Subsequent keys follow
the normal screen route. Resize, popup and help ownership still takes priority.
Saving, registration and draft connection tests refuse unfinished composition,
including blurred mapping fields. A focused comparison filter cannot confirm
sync while composing; confirm again after ending composition or deliberately
focus its native confirmation button. Native labelled buttons keep their actions.

Stored host/project values containing these characters block opening the form
before native normalization. Repair them outside this GUI form; drift does not
rewrite the configuration or secrets automatically.

## Remaining port work

Milestone 2 is in progress: automatic offers for matching endpoints, GUI preferences
and the planned Monokai themes remain. Project CRUD/archive, dashboard and startup
restoration and core project/host keyboard flows are available, including numeric
project shortcuts and the sync-error display shortcut. CLI project management and open/dash/version are
also available, along with opt-in file logging and visible logging failures.
Browser context menus, session-only resizable panes, preview/focus/copy shortcuts,
scrollable help and guarded native confirmation controls are available.
SFTP transport/browser and comparison/unified diff are available, including serial upload/download/delete sync. FTP now uses these same
workflows, including FTPS and certificate challenges. Finder fuzzy matching and
explicit return-state restoration and mapped remote ignored visibility are available.
Host/mapping controls have external-scroll focus reveal; Tools/Links/project-form
details/actions are bounded and wrapping. Single-line input boundaries reject
whole unsafe insertions and isolate IME Escape/Enter; native platform acceptance
remains. Character-precise, cross-line diff selection is implemented.
Packaging, release version injection and native release acceptance remain. Blocking local filesystem calls already running cannot
be interrupted by Tokio. Sync waits for their outcomes before reporting completion
or cancellation; browsing discards stale results. Concurrency remains bounded.
