package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

// Load merges the global config with the store of the project rooted at root.
//
// slug identifies that store; an empty slug means the caller found no
// registered project for the directory it started in, so only global hosts are
// available. Resolving a directory to a project is the caller's job — the
// registry lives in internal/project, which imports this package.
func Load(root, slug string) (*MergedConfig, error) {
	global, err := loadGlobal()
	if err != nil {
		return nil, err
	}

	var project *ProjectConfig
	if slug != "" {
		if project, err = loadProjectStore(slug); err != nil {
			return nil, fmt.Errorf("project store: %w", err)
		}
	}

	for _, host := range global.Hosts {
		if err := ValidateMappings(host.Mappings); err != nil {
			return nil, fmt.Errorf("global host %q mappings: %w", host.Name, err)
		}
	}
	if project != nil {
		if err := ValidateMappings(project.Mappings); err != nil {
			return nil, fmt.Errorf("project mappings: %w", err)
		}
		for _, host := range project.Hosts {
			if err := ValidateMappings(host.Mappings); err != nil {
				return nil, fmt.Errorf("project host %q mappings: %w", host.Name, err)
			}
		}
	}

	merged := merge(global, project, root)
	merged.ProjectSlug = slug
	merged.LegacyFiles = hasLegacyFiles(root)

	return merged, nil
}

// loadGlobal reads ~/.config/drift/config.toml.
// Returns an empty config if the file does not exist.
func loadGlobal() (*GlobalConfig, error) {
	path := globalConfigPath()
	cfg := &GlobalConfig{Defaults: Defaults{Port: 22}}

	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) {
		return cfg, nil
	}

	if _, err := toml.DecodeFile(path, cfg); err != nil {
		return nil, err
	}
	return cfg, nil
}

// merge combines global and project configs. Project hosts override global hosts by name.
func merge(global *GlobalConfig, project *ProjectConfig, projectRoot string) *MergedConfig {
	hosts := make(map[string]Host)

	applyDefaults := func(h Host, d Defaults) Host {
		if h.Port == 0 {
			if d.Port != 0 {
				h.Port = d.Port
			} else {
				h.Port = 22
			}
		}
		if h.User == "" {
			h.User = d.User
		}
		return h
	}

	globalHosts := make([]Host, 0, len(global.Hosts))
	for _, h := range global.Hosts {
		h = applyDefaults(h, global.Defaults)
		hosts[h.Name] = h
		globalHosts = append(globalHosts, h)
	}

	merged := &MergedConfig{
		GlobalDefaults: global.Defaults,
		UI:             global.UI,
		Hosts:          hosts,
		GlobalHosts:    globalHosts,
		ProjectHosts:   []Host{},
		ProjectRoot:    projectRoot,
	}

	if project == nil {
		return merged
	}

	projectHosts := make([]Host, 0, len(project.Hosts))
	for _, h := range project.Hosts {
		h = applyDefaults(h, project.Defaults)
		hosts[h.Name] = h
		projectHosts = append(projectHosts, h)
	}
	merged.ProjectDefaults = project.Defaults
	merged.ProjectHosts = projectHosts
	merged.Mappings = project.Mappings

	return merged
}

// Dir returns drift's configuration directory, honouring $XDG_CONFIG_HOME.
// It is the parent of both config.toml and projects.toml.
func Dir() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return filepath.Join(xdg, "drift")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config", "drift")
}

func globalConfigPath() string {
	return filepath.Join(Dir(), "config.toml")
}
