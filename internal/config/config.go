// Package config defines drift's configuration types and loading logic.
//
// Config resolution order:
//  1. ~/.config/drift/config.toml            (global hosts)
//  2. ~/.config/drift/projects/<slug>.toml   (a project's hosts + mappings)
//
// Nothing is stored in the project directory. Which project a directory belongs
// to is the registry's answer, so the caller passes the slug in.
// Project hosts with the same name override global hosts.
// Env vars in auth fields ($VAR) are expanded at connection time.
package config

// Host represents a remote target. Empty strings, false and empty lists are
// omitted from a written file, so a project config reads as the small,
// reviewable description of an environment it is rather than as a form with
// every blank filled in.
type Host struct {
	Name        string    `toml:"name"`               // unique identifier, e.g. "prod"
	Hostname    string    `toml:"hostname,omitempty"` // IP or domain
	Port        int       `toml:"port,omitempty"`     // default: 22 (sftp) or 21 (ftp)
	User        string    `toml:"user,omitempty"`
	Auth        Auth      `toml:"auth,omitempty"`
	RootPath    string    `toml:"root_path,omitempty"`    // remote base directory
	Protocol    string    `toml:"protocol,omitempty"`     // "sftp" (default), "ftp", or "ftps" (FTP over explicit TLS)
	InsecureTLS bool      `toml:"insecure_tls,omitempty"` // ftps: skip TLS certificate verification (self-signed certs)
	Mappings    []Mapping `toml:"mappings,omitempty"`     // per-host path mappings
}

// Auth configures how to authenticate with a Host. The credential fields are
// omitted when empty: for a project host they usually are, because literal
// passwords and passphrases live in the secret store, not in the project.
type Auth struct {
	Type       string `toml:"type,omitempty"`       // "password" | "keyfile" | "agent"
	Password   string `toml:"password,omitempty"`   // supports $ENV_VAR references
	KeyFile    string `toml:"key_file,omitempty"`   // path, ~ expanded at connect time
	Passphrase string `toml:"passphrase,omitempty"` // supports $ENV_VAR references
}

// Mapping maps a local directory/file to a remote path.
type Mapping struct {
	Local  string `toml:"local"`  // relative to project root
	Remote string `toml:"remote"` // relative to Host.RootPath
}

// Defaults provides fallback values for hosts that omit optional fields.
type Defaults struct {
	Port int    `toml:"port,omitempty"` // default 22
	User string `toml:"user,omitempty"`
}

// UI holds terminal interface preferences. Global config only — these describe
// how the terminal behaves, not anything project-specific.
type UI struct {
	// Mouse enables mouse reporting. Nil means unset; the default is enabled.
	// A pointer is needed to tell "absent from the file" from an explicit false.
	Mouse *bool `toml:"mouse,omitempty"`
}

// GlobalConfig is the structure of ~/.config/drift/config.toml.
type GlobalConfig struct {
	Defaults Defaults `toml:"defaults"`
	UI       UI       `toml:"ui,omitempty"`
	Hosts    []Host   `toml:"hosts"`
}

// ProjectConfig is the structure of a project store, <config.Dir()>/projects/<slug>.toml.
type ProjectConfig struct {
	Defaults Defaults  `toml:"defaults"`
	Hosts    []Host    `toml:"hosts"`
	Mappings []Mapping `toml:"mappings"`
}

// HostScope indicates whether a host was defined globally or in a project config.
type HostScope int

const (
	ScopeGlobal HostScope = iota
	ScopeProject
)

// MergedConfig is the runtime-resolved configuration after merging global and project configs.
type MergedConfig struct {
	GlobalDefaults  Defaults
	ProjectDefaults Defaults
	UI              UI              // from the global config only
	GlobalHosts     []Host          // hosts from ~/.config/drift/config.toml
	ProjectHosts    []Host          // hosts from the project store
	Hosts           map[string]Host // merged view: project overrides global, keyed by Name
	Mappings        []Mapping
	ProjectRoot     string // the project directory, from the registry

	// ProjectSlug is the registry slug of the open project, and with it the
	// name of its store under <config.Dir()>/projects/. Empty when the caller
	// found no registered project: there are only global hosts then, and
	// nothing to write.
	ProjectSlug string
}

// MouseEnabled reports whether mouse reporting should be turned on.
// Unset means enabled; the config can only turn it off.
func (c *MergedConfig) MouseEnabled() bool {
	if c == nil || c.UI.Mouse == nil {
		return true
	}
	return *c.UI.Mouse
}
