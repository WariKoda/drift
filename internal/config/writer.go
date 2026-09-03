package config

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

// SaveGlobalHost adds or replaces a host in the global config file.
// If oldName != "" the host with that name is replaced; otherwise a new host is appended.
func SaveGlobalHost(cfg *MergedConfig, h Host, oldName string) error {
	if err := ValidateMappings(h.Mappings); err != nil {
		return fmt.Errorf("host %q mappings: %w", h.Name, err)
	}
	hosts := replaceOrAppend(cfg.GlobalHosts, h, oldName)
	cfg.GlobalHosts = hosts
	rebuildMerged(cfg)
	return writeGlobal(GlobalConfig{Defaults: cfg.GlobalDefaults, UI: cfg.UI, Hosts: hosts})
}

// DeleteGlobalHost removes a host by name from the global config file.
func DeleteGlobalHost(cfg *MergedConfig, name string) error {
	cfg.GlobalHosts = removeHost(cfg.GlobalHosts, name)
	rebuildMerged(cfg)
	return writeGlobal(GlobalConfig{Defaults: cfg.GlobalDefaults, UI: cfg.UI, Hosts: cfg.GlobalHosts})
}

// SaveProjectHost adds or replaces a host in the project config file. Its
// access fields — user, auth, insecure TLS — are diverted to the access store,
// so the file drift writes into the project describes the environment only.
func SaveProjectHost(cfg *MergedConfig, h Host, oldName string) error {
	if err := ValidateMappings(h.Mappings); err != nil {
		return fmt.Errorf("host %q mappings: %w", h.Name, err)
	}
	if err := ValidateMappings(cfg.Mappings); err != nil {
		return fmt.Errorf("project mappings: %w", err)
	}

	base, err := projectFileBase(cfg)
	if err != nil {
		return err
	}

	environment, access := splitAccess(h, cfg.ProjectRoot)
	if err := storeAccess(access, oldName); err != nil {
		return fmt.Errorf("access store: %w", err)
	}

	// The in-memory host keeps its access fields — the session connects with it.
	cfg.ProjectHosts = replaceOrAppend(cfg.ProjectHosts, h, oldName)
	rebuildMerged(cfg)

	base.Hosts = replaceOrAppend(base.Hosts, environment, oldName)
	return writeProject(base, cfg.ProjectRoot)
}

// DeleteProjectHost removes a host by name from the project config file and
// drops its stored access.
func DeleteProjectHost(cfg *MergedConfig, name string) error {
	base, err := projectFileBase(cfg)
	if err != nil {
		return err
	}
	if err := deleteAccess(cfg.ProjectRoot, name); err != nil {
		return fmt.Errorf("access store: %w", err)
	}
	cfg.ProjectHosts = removeHost(cfg.ProjectHosts, name)
	rebuildMerged(cfg)

	base.Hosts = removeHost(base.Hosts, name)
	return writeProject(base, cfg.ProjectRoot)
}

// projectFileBase is the project config a write starts from: the file as it is
// on disk, not the merged view in memory. The merged view has defaults applied
// and access filled in from the store, so writing it back would bake
// [defaults] into every host record and copy this machine's access into a file
// the team shares.
//
// Without a file yet, the in-memory values are the only source — their access
// fields are stripped, since those belong to the store.
func projectFileBase(cfg *MergedConfig) (ProjectConfig, error) {
	pc, err := decodeProjectConfig(cfg.ProjectRoot)
	if err != nil {
		return ProjectConfig{}, fmt.Errorf("project config: %w", err)
	}
	if pc != nil {
		return *pc, nil
	}
	return ProjectConfig{
		Defaults: cfg.ProjectDefaults,
		Hosts:    environmentsOf(cfg.ProjectHosts, cfg.ProjectRoot),
		Mappings: cfg.Mappings,
	}, nil
}

// environmentsOf is hosts without their access fields.
func environmentsOf(hosts []Host, projectRoot string) []Host {
	out := make([]Host, len(hosts))
	for i, h := range hosts {
		out[i], _ = splitAccess(h, projectRoot)
	}
	return out
}

// rebuildMerged reconstructs cfg.Hosts from GlobalHosts + ProjectHosts.
func rebuildMerged(cfg *MergedConfig) {
	m := make(map[string]Host, len(cfg.GlobalHosts)+len(cfg.ProjectHosts))
	for _, h := range cfg.GlobalHosts {
		m[h.Name] = h
	}
	for _, h := range cfg.ProjectHosts {
		m[h.Name] = h // project overrides global
	}
	cfg.Hosts = m
}

func replaceOrAppend(hosts []Host, h Host, oldName string) []Host {
	if oldName == "" {
		return append(hosts, h)
	}
	result := make([]Host, 0, len(hosts))
	replaced := false
	for _, existing := range hosts {
		if existing.Name == oldName {
			result = append(result, h)
			replaced = true
		} else {
			result = append(result, existing)
		}
	}
	if !replaced {
		result = append(result, h)
	}
	return result
}

func removeHost(hosts []Host, name string) []Host {
	result := make([]Host, 0, len(hosts))
	for _, h := range hosts {
		if h.Name != name {
			result = append(result, h)
		}
	}
	return result
}

func writeGlobal(cfg GlobalConfig) error {
	for _, host := range cfg.Hosts {
		if err := ValidateMappings(host.Mappings); err != nil {
			return fmt.Errorf("host %q mappings: %w", host.Name, err)
		}
	}
	path := globalConfigPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	return writeToml(path, globalConfigOut{
		Defaults: defaultsOut{Port: optionalInt(cfg.Defaults.Port), User: cfg.Defaults.User},
		UI:       cfg.UI,
		Hosts:    hostsOut(cfg.Hosts),
	})
}

func writeProject(cfg ProjectConfig, projectRoot string) error {
	if err := ValidateMappings(cfg.Mappings); err != nil {
		return fmt.Errorf("project mappings: %w", err)
	}
	for _, host := range cfg.Hosts {
		if err := ValidateMappings(host.Mappings); err != nil {
			return fmt.Errorf("host %q mappings: %w", host.Name, err)
		}
	}
	dir := filepath.Join(projectRoot, ".drift")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	if err := ensureProjectGitignore(dir); err != nil {
		return err
	}
	return writeToml(filepath.Join(dir, "config.toml"), projectConfigOut{
		Defaults: defaultsOut{Port: optionalInt(cfg.Defaults.Port), User: cfg.Defaults.User},
		Hosts:    hostsOut(cfg.Hosts),
		Mappings: cfg.Mappings,
	})
}

// ensureProjectGitignore creates .drift/.gitignore if it does not exist yet.
// The project config carries credentials, so it must never be committed;
// other files under .drift/ stay shareable on purpose.
func ensureProjectGitignore(dir string) error {
	path := filepath.Join(dir, ".gitignore")
	if _, err := os.Stat(path); err == nil {
		return nil
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return os.WriteFile(path, []byte(projectGitignore), 0o600)
}

const projectGitignore = `# Written by drift: this file holds host credentials.
config.toml
`

// Encoding-only mirrors of the config types. BurntSushi's `omitempty` skips
// empty strings, false and empty lists, but not a numeric zero — a host whose
// port nobody set would be written back as `port = 0`. Since drift rewrites
// project configs that humans and teams maintain, it must trim those files, not
// decorate them.
type hostOut struct {
	Name        string    `toml:"name"`
	Hostname    string    `toml:"hostname,omitempty"`
	Port        *int      `toml:"port,omitempty"`
	User        string    `toml:"user,omitempty"`
	Auth        Auth      `toml:"auth,omitempty"`
	RootPath    string    `toml:"root_path,omitempty"`
	Protocol    string    `toml:"protocol,omitempty"`
	InsecureTLS bool      `toml:"insecure_tls,omitempty"`
	Mappings    []Mapping `toml:"mappings,omitempty"`
}

type defaultsOut struct {
	Port *int   `toml:"port,omitempty"`
	User string `toml:"user,omitempty"`
}

type projectConfigOut struct {
	Defaults defaultsOut `toml:"defaults,omitempty"`
	Hosts    []hostOut   `toml:"hosts,omitempty"`
	Mappings []Mapping   `toml:"mappings,omitempty"`
}

type globalConfigOut struct {
	Defaults defaultsOut `toml:"defaults,omitempty"`
	UI       UI          `toml:"ui,omitempty"`
	Hosts    []hostOut   `toml:"hosts,omitempty"`
}

func optionalInt(v int) *int {
	if v == 0 {
		return nil
	}
	return &v
}

func hostsOut(hosts []Host) []hostOut {
	out := make([]hostOut, len(hosts))
	for i, h := range hosts {
		out[i] = hostOut{
			Name:        h.Name,
			Hostname:    h.Hostname,
			Port:        optionalInt(h.Port),
			User:        h.User,
			Auth:        h.Auth,
			RootPath:    h.RootPath,
			Protocol:    h.Protocol,
			InsecureTLS: h.InsecureTLS,
			Mappings:    h.Mappings,
		}
	}
	return out
}

// writeToml encodes v and replaces path atomically: a temporary file in the
// same directory, then a rename. A crash or a full disk therefore leaves either
// the old file or the new one, never a truncated one — access.toml holds every
// project's credentials, and half of it is worse than none.
//
// It also makes two drift instances writing the store at the same time lose one
// of the two writes instead of interleaving into a broken file.
func writeToml(path string, v any) error {
	var buf bytes.Buffer
	if err := toml.NewEncoder(&buf).Encode(v); err != nil {
		return err
	}

	tmp, err := os.CreateTemp(filepath.Dir(path), "."+filepath.Base(path)+".*")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name()) // no-op once the rename succeeded

	if err := tmp.Chmod(0o600); err != nil {
		tmp.Close()
		return err
	}
	if _, err := tmp.Write(buf.Bytes()); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), path)
}
