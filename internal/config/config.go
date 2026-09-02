// Package config defines drift's configuration types and loading logic.
//
// Config resolution order:
//  1. ~/.config/drift/config.toml  (global hosts)
//  2. .drift/config.toml           (project hosts + mappings, walked up from cwd)
//
// Project config hosts with the same name override global hosts.
// Env vars in auth fields ($VAR) are expanded at connection time.
package config

// Host represents a remote SFTP/SSH or FTP target.
type Host struct {
	Name        string    `toml:"name"`     // unique identifier, e.g. "prod"
	Hostname    string    `toml:"hostname"` // IP or domain
	Port        int       `toml:"port"`     // default: 22 (sftp) or 21 (ftp)
	User        string    `toml:"user"`
	Auth        Auth      `toml:"auth"`
	RootPath    string    `toml:"root_path"`              // remote base directory
	Protocol    string    `toml:"protocol"`               // "sftp" (default), "ftp", or "ftps" (FTP over explicit TLS)
	InsecureTLS bool      `toml:"insecure_tls,omitempty"` // ftps: skip TLS certificate verification (self-signed certs)
	Mappings    []Mapping `toml:"mappings,omitempty"`     // per-host path mappings
}

// Auth configures how to authenticate with a Host.
type Auth struct {
	Type       string `toml:"type"`       // "password" | "keyfile" | "agent"
	Password   string `toml:"password"`   // supports $ENV_VAR references
	KeyFile    string `toml:"key_file"`   // path, ~ expanded at connect time
	Passphrase string `toml:"passphrase"` // supports $ENV_VAR references
}

// Mapping maps a local directory/file to a remote path.
type Mapping struct {
	Local  string `toml:"local"`  // relative to project root
	Remote string `toml:"remote"` // relative to Host.RootPath
}

// Defaults provides fallback values for hosts that omit optional fields.
type Defaults struct {
	Port int    `toml:"port"` // default 22
	User string `toml:"user"`
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

// ProjectConfig is the structure of .drift/config.toml.
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
	UI              UI // from the global config only
	GlobalHosts     []Host          // hosts from ~/.config/drift/config.toml
	ProjectHosts    []Host          // hosts from .drift/config.toml
	Hosts           map[string]Host // merged view: project overrides global, keyed by Name
	Mappings        []Mapping
	ProjectRoot     string // absolute path of the directory containing .drift/
}

// MouseEnabled reports whether mouse reporting should be turned on.
// Unset means enabled; the config can only turn it off.
func (c *MergedConfig) MouseEnabled() bool {
	if c == nil || c.UI.Mouse == nil {
		return true
	}
	return *c.UI.Mouse
}
