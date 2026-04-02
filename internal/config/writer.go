package config

import (
	"bytes"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

// SaveGlobalHost adds or replaces a host in the global config file.
// If oldName != "" the host with that name is replaced; otherwise a new host is appended.
func SaveGlobalHost(cfg *MergedConfig, h Host, oldName string) error {
	hosts := replaceOrAppend(cfg.GlobalHosts, h, oldName)
	cfg.GlobalHosts = hosts
	rebuildMerged(cfg)
	return writeGlobal(GlobalConfig{Hosts: hosts})
}

// DeleteGlobalHost removes a host by name from the global config file.
func DeleteGlobalHost(cfg *MergedConfig, name string) error {
	cfg.GlobalHosts = removeHost(cfg.GlobalHosts, name)
	rebuildMerged(cfg)
	return writeGlobal(GlobalConfig{Hosts: cfg.GlobalHosts})
}

// SaveProjectHost adds or replaces a host in the project config file.
func SaveProjectHost(cfg *MergedConfig, h Host, oldName string) error {
	hosts := replaceOrAppend(cfg.ProjectHosts, h, oldName)
	cfg.ProjectHosts = hosts
	rebuildMerged(cfg)
	return writeProject(ProjectConfig{Hosts: hosts, Mappings: cfg.Mappings}, cfg.ProjectRoot)
}

// DeleteProjectHost removes a host by name from the project config file.
func DeleteProjectHost(cfg *MergedConfig, name string) error {
	cfg.ProjectHosts = removeHost(cfg.ProjectHosts, name)
	rebuildMerged(cfg)
	return writeProject(ProjectConfig{Hosts: cfg.ProjectHosts, Mappings: cfg.Mappings}, cfg.ProjectRoot)
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
	path := globalConfigPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return writeToml(path, cfg)
}

func writeProject(cfg ProjectConfig, projectRoot string) error {
	dir := filepath.Join(projectRoot, ".drift")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	return writeToml(filepath.Join(dir, "config.toml"), cfg)
}

func writeToml(path string, v any) error {
	var buf bytes.Buffer
	if err := toml.NewEncoder(&buf).Encode(v); err != nil {
		return err
	}
	return os.WriteFile(path, buf.Bytes(), 0o644)
}
