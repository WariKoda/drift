package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

// A project's configuration lives entirely under <config.Dir()>/projects/,
// one file per project, named after its registry slug:
//
//	~/.config/drift/projects/kalieber.toml
//
// Nothing is written into the project directory. That is why the store is keyed
// by slug and not by path: the registry owns the path, so moving a project keeps
// its hosts as long as the registry is updated.
//
// The file holds hosts complete with their credentials, so it is 0600 in a 0700
// directory.

func projectsDir() string {
	return filepath.Join(Dir(), "projects")
}

// projectStorePath is the store file for slug. The slug comes from
// project.Slugify, but this is the one place it becomes a file name, so it is
// checked rather than trusted.
func projectStorePath(slug string) (string, error) {
	if slug == "" {
		return "", errors.New("no project slug")
	}
	if slug != filepath.Base(slug) || slug == "." || slug == ".." || strings.ContainsRune(slug, filepath.Separator) {
		return "", errors.New("project slug is not a usable file name: " + slug)
	}
	return filepath.Join(projectsDir(), slug+".toml"), nil
}

// loadProjectStore reads a project's store. A project without one yields
// (nil, nil): it simply has no hosts of its own yet.
func loadProjectStore(slug string) (*ProjectConfig, error) {
	path, err := projectStorePath(slug)
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	cfg := &ProjectConfig{}
	if _, err := toml.DecodeFile(path, cfg); err != nil {
		return nil, err
	}
	return cfg, nil
}

// writeProjectStore replaces a project's store.
func writeProjectStore(slug string, cfg ProjectConfig) error {
	if err := ValidateMappings(cfg.Mappings); err != nil {
		return fmt.Errorf("project mappings: %w", err)
	}
	for _, host := range cfg.Hosts {
		if err := ValidateMappings(host.Mappings); err != nil {
			return fmt.Errorf("host %q mappings: %w", host.Name, err)
		}
	}
	path, err := projectStorePath(slug)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	return writeToml(path, projectConfigOut{
		Defaults: defaultsOut{Port: optionalInt(cfg.Defaults.Port), User: cfg.Defaults.User},
		Hosts:    hostsOut(cfg.Hosts),
		Mappings: cfg.Mappings,
	})
}

// ProjectStorePathForDisplay is a project's store path with $HOME shortened,
// for messages that tell the user where its configuration went.
func ProjectStorePathForDisplay(slug string) string {
	path, err := projectStorePath(slug)
	if err != nil {
		return projectsDir()
	}
	home, herr := os.UserHomeDir()
	if herr != nil || home == "" {
		return path
	}
	if rel := strings.TrimPrefix(path, home); rel != path {
		return "~" + rel
	}
	return path
}
