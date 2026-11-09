# Comparing and syncing

## Typical workflow

1. Run `drift` in your project directory.
2. Mark one or more files or directories with `Space`.
3. Press `s` and choose a host.
4. Review the diffs and the suggested sync directions.
5. Sync the current file with `s` or all files with `S`.

While a connect or sync overlay is up, `Esc` hides it. The work keeps going, with
`[q] cancel` on the status line. `q` / `Ctrl+C` abort it. Files that were already
transferred stay; the rest is skipped.

## What gets compared

Hidden and gitignored entries are hidden in the browser by default. `.` and `I`
toggle them independently; they do not change the sync scope.

- A recursively selected directory includes hidden files such as `.htaccess` and
  `.well-known`, but skips gitignored paths.
- drift applies the local project's ignore rules to both local and mapped remote
  files, so an ignored remote-only file does not become a local delete or download
  candidate.
- Selecting an ignored file directly includes that file. Selecting an ignored
  directory does not include its contents.
- `.git`, `.svn`, `.hg`, `node_modules`, `.idea`, and `.vscode` directories are
  always excluded, even when ignored files are included. Symlinks and paths outside
  the project or active mappings keep their safety checks.

drift reads the ignore rules through `git check-ignore`, so nested rules, tracked
files, worktrees, `.git/info/exclude` and global excludes behave as in Git.

The diff status line shows the number of pairs, hidden files, skipped ignored paths,
and fixed exclusions. Press `i` to rebuild that comparison with all gitignored paths
included for this operation. drift reloads the rules and comparison; the setting does
not change the global config.

When a host or project has [path mappings](configuration.md#path-mappings), only files
under a mapping can be synced.

## The diff view

The file list groups comparisons by directory, relative to the project (or the host
root path for files outside it). Each file shows:

- its comparison status: `~` changed, `L` local only, `R` remote only, `B` binary,
  `!` error;
- its added/removed line counts, in the same before/after order as the diff;
- the planned sync action in the last column.

Status and action stay separate: "local only" describes the comparison, the action is
your decision. drift pre-selects the action from the modification times. `Space`
cycles it for the current file, `A` for all files.

The diff pane opens with a one-line title: the relative path, a status label such as
`remote only`, the line counts, and the before → after orientation of the line numbers
(`Compare only` when no action is planned). `f` adds the full local and remote paths.

Unchanged stretches are folded away behind `@@` hunk headers. Hunk headers name the
code around them when the comparison shows it: the nearest line before the hunk that
starts with a letter, `_` or `$` (Git's default rule), or the nearest heading in
Markdown files. Small changes within a replacement line are emphasized in addition to
the whole-line colors.

Hunk navigation with `[` and `]` crosses file boundaries: `]` enters the next file at
its first hunk, `[` enters the previous file at its last. Files without textual hunks
(identical, binary, and failed comparisons) are skipped. Navigation stops at either
end without wrapping. Tab and Shift+Tab reach every file and restore its saved reading
position.

A file with a changed line of 4 KiB or more, such as a minified bundle, opens as a
short summary; `o` shows its diff. This only affects the display: the file keeps its
sync action and is transferred like any other.

The diff keeps each visited file's reading position and expanded context. Refresh
(`r`) reconciles those positions with the new content; it never restores old transfer
decisions.

### All files in one view

`m` shows all files one below the other, each under its own title. Scrolling then runs
from one file into the next, and the file at the top becomes the current file for
`Space`, `s`, `u` and `d`. It is named in the fixed title above the diff and marked in
its section. Tab, a file list click or `]`/`[` pick the current file directly. `z` or
a click on a section title folds a file to its title.

### Layout and display settings

Unified is the default layout. `v` cycles Unified, Split and Auto. Auto switches to
split when at least 160 columns remain for the diff after the file list. Very narrow
windows hide the file list; Tab and Shift+Tab still select files. Layout changes last
for the session.

Set the initial values in `~/.config/drift/config.toml`:

```toml
[ui.diff]
layout = "auto"   # unified (default), split, or auto
syntax = true     # syntax highlighting; default false
continuous = true # start with all files in one view; default false
wrap = false      # clip long lines and scroll with ←/→; default true
```

Syntax highlighting uses Chroma on the source already present in the comparison. It
never reloads files or runs project code. Unknown languages, files exceeding its work
limits, and cancelled work keep the plain diff. See
[diff presentation](../diff-presentation.md) for limits and measurements.
