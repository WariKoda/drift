# Go TUI stability and performance fixes

Rust/GPUI work remains paused. This series addresses the subsequent Go TUI audit without changing `main` or writing configuration into a project working tree.

## Stacked branches

Review and merge in this order; each branch includes the preceding branches:

1. `fix/tui-terminal-and-modal-safety`
2. `fix/tui-session-state`
3. `fix/tui-layout-and-rendering`
4. `fix/tui-async-local-browser`

The series is based on `1af07c6`. No merge into `main` or push is part of this implementation.

## Changes

### Terminal output and overlays

- `styles.SafeText` visibly escapes terminal controls, bidi controls and Unicode line separators, and replaces invalid UTF-8. Apply it only to display strings: actual paths, credentials, transfer bytes and clipboard actions retain their original values.
- Browser entries, finder results, project/host labels, paths, error details, text inputs and unified-diff text use escaped display values. Existing preview/certificate sanitization remains in place.
- Browser help consumes keyboard input; `?`, Escape and `q` dismiss it without modifying marks. Ctrl+C still quits.
- Sync-error details consume input instead of triggering invisible transfers. Arrow keys/`j`/`k`, page keys/Ctrl+D/Ctrl+U, Home/End/`g`/`G` and the mouse wheel scroll the details. Long paths/reasons wrap rather than disappearing beyond an ellipsis. Resize clamps the error viewport.

### Session and selection state

- Successful host saves, deletes and link changes reconcile the assigned host against the resolved configuration. A changed or removed host invalidates pending comparisons and observers, detaches its connection and clears obsolete remote marks. The returned command closes the transport; no transfer is automatically retried. Unrelated host edits preserve the session.
- Saving an active project's new path prepares its replacement browser asynchronously. The old session survives until preparation succeeds. Name-only edits refresh the browser label. Choosing a project with the same slug but a different path no longer skips reopening it.
- Confirmed local/remote deletions remove marks from both comparison snapshots and live browser selections. Bulk commands also report immutable confirmed deletion paths, so completed operations are reconciled even after cancellation or connection loss.
- Returning from comparison restores live selection ownership, clears marks and refreshes the local tree. Completed transfers also schedule local refreshes.

### Display cells and remote rendering

- Text fields size placeholders, contents and scrolling in terminal cells, not rune counts. Cursor movement/deletion respects grapheme clusters, including combining characters and joined emoji. The cursor style lives in the shared style package.
- Browser indentation is bounded by pane width. Names, path tails, diff content and project-list columns use terminal-cell-aware truncation. Remote placeholders/loading messages cannot displace the divider.
- The remote visible-entry projection is calculated once per browser frame and passed into row rendering.

`BenchmarkLargeRemoteView` renders 10,000 remote entries in a 120-column, 54-row terminal. Three local runs before and after the rendering change produced these median measurements:

| Metric | Before | After |
| --- | ---: | ---: |
| Time/frame | 1.766 ms | 0.154 ms |
| Bytes/frame | 4,021,613 | 136,611 |
| Allocations/frame | 855 | 797 |

This is approximately 11.5× faster rendering and 96.6% fewer allocated bytes in this specific synthetic benchmark, not an end-to-end transfer benchmark. The baseline used a Go overlay with the previous browser renderer and matching existing tests; the benchmark itself was unchanged.

### Asynchronous local browsing

- Directory expansion and refresh execute directory reads and Git classification in `tea.Cmd` commands. Commands receive paths, classifier handles and immutable expansion snapshots, never the live tree entries.
- Each request has a browser-session identity and unique request identity. Collapsing, cancelling, refreshing again or replacing a project invalidates superseded results.
- Refresh replaces the tree/classifier atomically after success. Expanded hierarchy, current path, visibility settings and marks survive successful refreshes; failures retain the previous tree and are logged and displayed.
- Navigation and marking remain available during reads. Escape cancels pending local reads without clearing marks. Explicit expansion/collapse supersedes a pending refresh so its old snapshot cannot undo newer tree interaction.
- The root routes local results even when a host/project modal is open and cancels pending reads on project replacement/shutdown. Configuration assignment no longer rereads an already initialized local tree.
- Classifier construction locates the Git worktree without scanning project descendants. Directory reads and batched ignore checks run on demand, so an unrelated unreadable directory does not prevent opening the project. Refresh reclassifies marked paths even when their directories are collapsed or they exist only remotely; marks added during refresh trigger a follow-up request before the new snapshot is adopted. Cancellation reaches Git subprocesses, but an individual blocked OS filesystem call cannot be forcibly interrupted; it no longer blocks `Update`, and its stale result is discarded.

## Validation

Permanent regression tests use real local files and the existing in-process FTP server, not remote mocks. They cover the audit findings plus asynchronous request ownership, cancellation, refresh failures, preserved tree links, and confirmed deletions after cancellation.

```sh
go test ./...
go test -race -count=1 ./...
go vet ./...
go build ./...
go test -run '^$' -bench '^BenchmarkLargeRemoteView$' -count=3 ./internal/tui/browser
```

The original temporary audit regressions were also rerun successfully. Their tree-expansion setup was updated to execute the new asynchronous command, and their stale-host test executes the returned transport-close command.

Manual acceptance should include unusually named files, a narrow/deep local tree, CJK/emoji form input, a long sync-error list, host root/mapping edits, changing an active project path, and expansion/refresh on a slow filesystem. Automated validation does not substitute for an interactive terminal smoke test.
