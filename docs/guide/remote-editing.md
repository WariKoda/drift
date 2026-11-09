# Editing remote files

Connect to a host with `@`, switch to the remote pane with `Tab`, select a regular
file and press `e`. Configure one editor in `~/.config/drift/config.toml` first:

```toml
[editor]
command = ["nano", "{file}"]
terminal = true
```

Other examples, replacing the `command` and `terminal` values above:

| Editor | `command` | `terminal` |
| --- | --- | --- |
| Neovim | `["nvim", "{file}"]` | `true` |
| Zed | `["zed", "--wait", "{file}"]` | `false` |
| VS Code | `["code", "--wait", "{file}"]` | `false` |

`{file}` must be a separate argument and appear exactly once. Commands run without
a shell, from the private working directory. `terminal` defaults to `true`. GUI
editors must wait until the file is closed; a launcher without a wait option is
not supported. Editor commands are global user preferences, never project or host
settings. There is no automatic `$EDITOR`/`$VISUAL` fallback.

## What happens

drift downloads the original remote bytes to a private directory in the OS user
cache (`~/.cache/drift/remote-edit/` on Linux unless `$XDG_CACHE_HOME` is set).
The local project file is not used or changed. Only one file is edited at a time,
up to 10 MiB, within the active mappings or the host root. A terminal editor takes
over the terminal; a GUI editor opens separately while drift waits.

Saving in the editor does **not** upload. After you close the file or editor:

- unchanged bytes return directly to the browser;
- changed bytes require `u` to upload; `Enter`/`e` opens the editor again;
- `k` keeps the local copy and returns; `d` asks before discarding it;
- `c` reconnects and checks content without uploading; `r` reloads remote content
  only after you confirm discarding the working changes.

The browser keeps its cursor, open directories and markings when you return.

## Conflicts and failures

Before upload, drift reads the remote content again. Parallel changes, missing files
and file-type changes stop the upload. There is no force-overwrite or automatic merge.
This is not an atomic conditional write: another process can still change the remote
file between the check and the server-side replacement. Uploads use the normal staged
transfer; server rename capabilities and FTP/FTPS metadata preservation vary.

Failures preserve the working copy. An interrupted upload can have an unknown
outcome; reconnecting checks it before any new, separately confirmed upload.
drift removes the working copy after a confirmed save or discard and after an
unchanged session, unless later changes need keeping. Kept copies can contain
secrets: they have private permissions but are not encrypted or recovered on restart.
drift shows their path so you can retrieve and remove them yourself.
