package config

import (
	"errors"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

// Load finds and merges global and project configs relative to startDir.
// startDir is typically os.Getwd().
func Load(startDir string) (*MergedConfig, error) {
	global, err := loadGlobal()
	if err != nil {
		return nil, err
	}

	project, projectRoot, err := loadProject(startDir)
	if err != nil {
		return nil, err
	}

	return merge(global, project, projectRoot), nil
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

// loadProject walks from startDir upward looking for .drift/config.toml.
// Returns nil project config (no error) if none is found.
func loadProject(startDir string) (*ProjectConfig, string, error) {
	dir := startDir
	for {
		candidate := filepath.Join(dir, ".drift", "config.toml")
		if _, err := os.Stat(candidate); err == nil {
			cfg := &ProjectConfig{}
			if _, err := toml.DecodeFile(candidate, cfg); err != nil {
				return nil, "", err
			}
			return cfg, dir, nil
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			break // reached filesystem root
		}
		dir = parent
	}
	return nil, startDir, nil
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

// HasProjectContext reports whether startDir or any parent contains a
// .drift/config.toml — i.e. whether drift is being invoked inside a project.
func HasProjectContext(startDir string) bool {
	dir := startDir
	for {
		if _, err := os.Stat(filepath.Join(dir, ".drift", "config.toml")); err == nil {
			return true
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return false
		}
		dir = parent
	}
}
