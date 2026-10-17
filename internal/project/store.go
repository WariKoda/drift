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
	reg.original = append([]Project(nil), reg.Projects...)
	return reg, nil
}

// Save writes the registry, creating the config directory if needed. The write
// replaces the file atomically, so a full disk or a killed process leaves the
// previous registry in place instead of a truncated one that would lose every
// project.
func (s *Store) Save(reg *Registry) error {
	return config.WithWriteLock(func(lock *config.WriteLock) error { return s.SaveLocked(lock, reg) })
}

// SaveLocked merges only the caller's changes into a fresh registry. Unrelated
// changes survive; a changed record conflicts instead of silently overwriting.
// Use the caller's transaction during multi-file project removal.
func (s *Store) SaveLocked(lock *config.WriteLock, reg *Registry) error {
	if !lock.Valid() || s.path != defaultPath() {
		return errors.New("registry save requires its configuration write lock")
	}
	fresh, err := s.Load()
	if err != nil {
		return err
	}
	for _, before := range reg.original {
		after := reg.Find(before.Slug)
		if after != nil && before.Equal(*after) {
			continue
		}
		current := fresh.Find(before.Slug)
		if current == nil || !current.Equal(before) {
			return &config.ConflictError{Record: "project " + before.Slug}
		}
		if after == nil {
			if err := fresh.Remove(before.Slug); err != nil {
				return err
			}
		} else {
			if err := fresh.Update(before.Slug, *after); err != nil {
				return err
			}
		}
	}
	for _, after := range reg.Projects {
		existed := false
		for _, before := range reg.original {
			if before.Slug == after.Slug {
				existed = true
				break
			}
		}
		if !existed {
			if err := fresh.Add(after); err != nil {
				return err
			}
		}
	}
	if err := os.MkdirAll(filepath.Dir(s.path), 0o700); err != nil {
		return err
	}
	if err := config.WriteTOML(s.path, fresh); err != nil {
		return err
	}
	fresh.original = append([]Project(nil), fresh.Projects...)
	*reg = *fresh
	return nil
}
