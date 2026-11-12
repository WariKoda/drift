# Projects and the dashboard

drift keeps a registry of your projects (one per customer, say) and can show them in a
dashboard on startup. Selecting a project re-roots drift: it loads that project's
hosts and opens the file browser in its directory. This is the same as
`cd <path> && drift`, without leaving drift.

## Command line

```bash
# Start in the current directory, the last project, or the dashboard (see below)
drift

# Open the project dashboard explicitly
drift dash

# Open a registered project by name or slug
drift open kunde-a

# Manage projects
drift projects list
drift projects add "KUNDE A" ~/work/kunde-a
drift projects edit kunde-a --name "KUNDE A GmbH" --path ~/work/kunde-a
drift projects archive kunde-a
drift projects remove kunde-a

# Show version
drift version
```

`drift open` accepts a display name or slug: exact slug, then exact name
(case-insensitive), then a unique prefix or substring of either.

## Which screen drift starts on

- Inside a registered project directory, `drift` opens the file browser.
- Outside any registered project, `drift` reopens the project you opened last, as long
  as its path still exists.
- Otherwise it shows the dashboard, if at least one project is registered.

`--dashboard` (or `drift dash`) always opens the list. `--no-dashboard` stays in the
current directory.

## Switching projects

From the file browser, `P` opens a filterable switcher and leaves the current session
alone. Esc goes back. Enter on another project re-roots. `m` (with an empty filter)
opens the full dashboard to add, edit, archive, or remove entries.

On the dashboard, `/` filters projects by name, slug or path. `Enter` finishes
filter input; press it again to open the selected project. Number keys and
management actions use the filtered list. `Esc` clears the filter before leaving
the dashboard. Terminals shorter than 20 rows use a compact header to leave more
room for projects.

## Registering a directory

The registry is also how drift knows which project a directory belongs to: the
registered project whose path is the directory or a parent of it, longest match first.
Registering is therefore what gives a directory hosts of its own.

drift offers to do it for you. Start it in a repository that no project covers and it
asks, suggesting the repository root rather than whatever subdirectory you were in.
Press `y` to register, any other key to skip. Otherwise use `drift projects add .`, or
`n` on the dashboard, which prefills the repository root too.

`~/.config/drift/projects.toml` holds a slug, display name, local path and timestamps
per project. Its hosts and mappings live next to it in
`~/.config/drift/projects/<slug>.toml`.
