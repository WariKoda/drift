# Configuration

drift writes these files itself when you use the host manager and project forms. Nothing
is stored in your project directory: no `.drift/` directory, no dotfile, nothing to add
to `.gitignore`.

| File | Holds |
| --- | --- |
| `~/.config/drift/config.toml` | global servers, `[ui]`, `[editor]` |
| `~/.config/drift/projects.toml` | the registry: slug, name, path, timestamps |
| `~/.config/drift/projects/<slug>.toml` | one project's hosts and mappings, mode `600` |
| `~/.config/drift/trusted-certificates.toml` | persistent FTPS certificate exceptions, mode `600` |

`$XDG_CONFIG_HOME` replaces `~/.config` when it is set.

The trade-off is deliberate: mappings do not travel with a clone. Every developer enters
them once per machine, and when someone moves a directory in the repository there is no
commit that fixes the mapping for everyone.

## Global config: `~/.config/drift/config.toml`

Global hosts are servers. A project does not see them until one of its hosts links to
one (see [Sharing a server between projects](#sharing-a-server-between-projects)).
Without an open project, drift offers the global hosts directly.

```toml
[defaults]
user = "deploy"

[[hosts]]
name       = "prod"
hostname   = "example.com"
port       = 22
user       = "deploy"
root_path  = "/var/www/html"
protocol   = "sftp"

  [hosts.auth]
  type     = "keyfile"
  key_file = "~/.ssh/id_ed25519"

[ui]
show_hidden = true   # initial browser visibility; default false
show_ignored = true  # initial browser visibility; default false
```

`[ui.diff]` is described in [Comparing and syncing](sync.md#layout-and-display-settings),
`[editor]` in [Editing remote files](remote-editing.md), and `[ui] mouse` in
[Key bindings and mouse](keys.md#mouse).

## Project config: `~/.config/drift/projects/<slug>.toml`

drift writes this file; the slug comes from the registry.

```toml
[defaults]
user = "deploy"

[[hosts]]
name      = "staging"
hostname  = "shopdev.example.com"
port      = 21
user      = "webuser"
root_path = "/var/www"
protocol  = "ftps"

  [hosts.auth]
  type     = "password"
  password = "$DEPLOY_PASSWORD"

  [[hosts.mappings]]
  local  = "plugins/plugin1"
  remote = "html/custom/plugins/plugin1"

[[mappings]]
local  = "src"
remote = "html"
```

The file holds credentials verbatim, so it is mode `600` in a `700` directory. Use
`$ENV_VAR` for a password or passphrase to keep the secret in your shell environment or
a password manager; drift expands it at connect time.

## Auth types

| Type | Fields |
| ------ | -------- |
| `keyfile` | `key_file`, `passphrase` (optional) |
| `password` | `password` (supports `$ENV_VAR`) |
| `agent` | none, uses the SSH agent |

drift checks SFTP host keys against `~/.ssh/known_hosts`. It adds an unknown host's key
on first connect and rejects the connection if a known host's key changes.

## Path mappings

`local` paths are relative to the project root. `remote` paths are relative to the
host's `root_path`.

When mappings are configured, only files under a mapping can be synced; files outside
all mappings are excluded. Without mappings, all files sync relative to `root_path`.
Host-level `mappings` take precedence; the project-level `[[mappings]]` are the
fallback for hosts without their own.

## Sharing a server between projects

When several projects deploy to the same machine, keep the connection once as a global
server and let each project link to it. A link names the server and adds only what
differs per project:

```toml
# ~/.config/drift/projects/shop-b.toml
[[hosts]]
name      = "prod"
server    = "kunde-x"          # a host in ~/.config/drift/config.toml
root_path = "/var/www/shop-b"

  [[hosts.mappings]]
  local  = "public"
  remote = "htdocs"
```

Hostname, port, user, auth, protocol and keep-alive come from the server; drift rejects
a link that sets any of them. A changed password therefore needs one edit, and every
linked project uses it on its next connection. `root_path` and `mappings` stay with the
link, and the server's own mappings are not inherited.

In the host manager, press `l` (or `n` in the project section, then `l`) and pick a
server. The list also shows the hosts of your other projects. Picking one of those
turns it into a global server after you confirm, and its project keeps using it through
a link. When you save a project host whose hostname, port, user and protocol match a
server or another project's host, drift offers to link that one instead of saving a
second copy of the connection.

A server that projects still link cannot be deleted or renamed; the error names those
projects.

## Keep-alive

Remote connections send protocol keep-alive probes every 60 seconds by default. Set
`keep_alive_interval` on a host in seconds, or edit the field in the host form:

```toml
keep_alive_interval = 60 # 0 disables keep-alive; omit for the default
```

TOML accepts values from `0` to `86400`. In the host form, `Disable keep-alive` turns
probes off and hides the interval field; otherwise the field accepts `1` to `86400`
seconds, and a blank field uses the default. Changes apply to new connections. This is
a host setting, not a `[defaults]` field.

FTP and FTPS send `NOOP` only after an idle interval on the control connection, never
during a running transfer. SFTP sends SSH keep-alive requests, which can run alongside
transfers. A probe has 15 seconds to receive a response.

A failed probe marks the connection as disconnected. In the remote browser, use `r` to
reconnect; in the diff view, return to the browser and compare again before syncing.
drift never reconnects or repeats transfers automatically. An interrupted operation may
have an unknown result on the server.

Keep-alive cannot prevent every server idle limit or network failure.

## FTPS certificate trust

drift checks FTPS certificates against the operating system trust store, including the
hostname and validity period. If that check fails, drift stops the connection and shows
the certificate subject, issuer, validity, problem, and full SHA-256 fingerprint.
`Reject` is selected by default.

After checking the fingerprint through a trusted channel, you can trust that exact
certificate for the current drift process or permanently. Permanent exceptions are bound
to the normalized hostname and port and stored in
`~/.config/drift/trusted-certificates.toml`. A changed certificate or a new validation
problem asks again. Press `r` on an FTPS host in the host manager to reset both session
and persistent trust for its endpoint.

The former `insecure_tls` host option is ignored and no longer written. Custom CA files
and client certificates are not supported yet. FTPS uses explicit TLS on the FTP control
port and TLS 1.2.

## Theming

drift uses the terminal palette by default and detects Omarchy themes from
`$XDG_STATE_HOME/omarchy/current/theme/colors.toml` (normally
`~/.local/state/omarchy/current/theme/colors.toml`). The older location under
`$XDG_CONFIG_HOME` works as a fallback. Override the choice with:

```bash
DRIFT_THEME=auto|ansi|omarchy|default
DRIFT_THEME_FILE=/path/to/colors.toml
```

`auto` loads Omarchy colors first and falls back to ANSI terminal colors. `default` uses
drift's built-in palette.

## Logging

Logging is off by default. When enabled, drift writes diagnostics (connection lifecycle
and every connect, sync and diff error, with full paths) to a file, never to the
terminal. Enable it per run with a flag or environment variable; the flag wins:

```bash
drift --debug                 # debug level, ~/.config/drift/drift.log
DRIFT_DEBUG=1 drift           # same, via environment
drift --log /tmp/drift.log    # info level, explicit path
DRIFT_LOG=/tmp/drift.log drift
```

Debug level adds a line per synced file. drift appends to the log file and does not
rotate it. The `e` error list in the diff view holds only the most recent bulk sync;
the log keeps everything.

## Upgrading from 0.1.6-alpha or earlier

Those versions kept a `.drift/config.toml` in the project, with credentials either in it
or in `~/.config/drift/secrets.toml`. 0.1.7-alpha is the only release that moves them
into the project store; later versions no longer read those files.

Install [0.1.7-alpha](https://github.com/WariKoda/drift/releases/tag/v0.1.7-alpha) first
and start it once in each project. It migrates the hosts and reports what it moved. Then
upgrade to the current version. Skipping it leaves your hosts in files nothing reads, and
drift will look as if those projects have no hosts.

If such a config was ever committed, the password in it is in your repository's
history, where deleting the file later does not reach it. Rotate it.
