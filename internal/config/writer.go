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

// SaveProjectHost adds or replaces a host in the project config file. Literal
// credentials are diverted to the secret store, so the file drift writes into
// the project never contains one.
func SaveProjectHost(cfg *MergedConfig, h Host, oldName string) error {
	if err := ValidateMappings(h.Mappings); err != nil {
		return fmt.Errorf("host %q mappings: %w", h.Name, err)
	}
	if err := ValidateMappings(cfg.Mappings); err != nil {
		return fmt.Errorf("project mappings: %w", err)
	}

	_, secret := splitSecret(h, cfg.ProjectRoot)
	if err := storeSecret(secret, oldName); err != nil {
		return fmt.Errorf("secret store: %w", err)
	}

	// The in-memory host keeps its credentials — the session connects with it.
	cfg.ProjectHosts = replaceOrAppend(cfg.ProjectHosts, h, oldName)
	rebuildMerged(cfg)

	return writeProject(ProjectConfig{
		Defaults: cfg.ProjectDefaults,
		Hosts:    strippedHosts(cfg.ProjectHosts, cfg.ProjectRoot),
		Mappings: cfg.Mappings,
	}, cfg.ProjectRoot)
}

// DeleteProjectHost removes a host by name from the project config file and
// drops its stored credentials.
func DeleteProjectHost(cfg *MergedConfig, name string) error {
	if err := deleteSecret(cfg.ProjectRoot, name); err != nil {
		return fmt.Errorf("secret store: %w", err)
	}
	cfg.ProjectHosts = removeHost(cfg.ProjectHosts, name)
	rebuildMerged(cfg)
	return writeProject(ProjectConfig{
		Defaults: cfg.ProjectDefaults,
		Hosts:    strippedHosts(cfg.ProjectHosts, cfg.ProjectRoot),
		Mappings: cfg.Mappings,
	}, cfg.ProjectRoot)
}

// strippedHosts is hosts without their literal credentials, ready to be
// written into the project. The stored copies are written separately.
func strippedHosts(hosts []Host, projectRoot string) []Host {
	out := make([]Host, len(hosts))
	for i, h := range hosts {
		out[i], _ = splitSecret(h, projectRoot)
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
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return writeToml(path, cfg)
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
	return writeToml(filepath.Join(dir, "config.toml"), cfg)
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

func writeToml(path string, v any) error {
	var buf bytes.Buffer
	if err := toml.NewEncoder(&buf).Encode(v); err != nil {
		return err
	}
	return os.WriteFile(path, buf.Bytes(), 0o600)
}
