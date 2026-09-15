package project

import (
	"errors"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
	"github.com/WariKoda/drift/internal/config"
)

// Store persists the project Registry to a TOML file.
type Store struct {
	path string
}

// NewStore returns a Store backed by <config-dir>/projects.toml.
func NewStore() *Store {
	return &Store{path: defaultPath()}
}

// Path reports the file the Store reads from and writes to.
func (s *Store) Path() string { return s.path }

func defaultPath() string {
	return filepath.Join(config.Dir(), "projects.toml")
}

// Load reads the registry. A missing file yields an empty registry (no error);
// a malformed file returns an error and never silently discards data.
func (s *Store) Load() (*Registry, error) {
	reg := &Registry{}
	if _, err := os.Stat(s.path); errors.Is(err, os.ErrNotExist) {
		return reg, nil
	}
	if _, err := toml.DecodeFile(s.path, reg); err != nil {
		return nil, err
	}
	return reg, nil
}

// Save writes the registry, creating the config directory if needed. The write
// replaces the file atomically, so a full disk or a killed process leaves the
// previous registry in place instead of a truncated one that would lose every
// project.
func (s *Store) Save(reg *Registry) error {
	if err := os.MkdirAll(filepath.Dir(s.path), 0o755); err != nil {
		return err
	}
	return config.WriteTOML(s.path, reg)
}
