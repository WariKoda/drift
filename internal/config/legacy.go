package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

// Everything in this file exists to get configuration out of the places drift
// used to keep it and into the per-project store. It reads three legacy
// sources and writes none of them:
//
//	<project>/.drift/config.toml   hosts and mappings, with credentials before 0.1.6
//	<config.Dir()>/access.toml     per-project access (0.1.7)
//	<config.Dir()>/secrets.toml    per-project credentials (0.1.6)
//
// Once no installation has any of them left, this file and gitguard.go go away.

// legacyProjectConfig is <root>/.drift/config.toml, the file drift used to
// write into the project itself.
func legacyProjectConfig(root string) string {
	return filepath.Join(root, ".drift", "config.toml")
}

func accessPath() string  { return filepath.Join(Dir(), "access.toml") }
func secretsPath() string { return filepath.Join(Dir(), "secrets.toml") }

// projectGitignore is the file drift wrote next to the project config to keep
// it out of commits. The migration removes it again, but only if it is
// byte-identical to what drift wrote.
const projectGitignore = "# Written by drift: this file holds host credentials.\nconfig.toml\n"

// accessFile is the on-disk shape of access.toml.
type accessFile struct {
	Access []hostAccess `toml:"access"`
}

// hostAccess is one person's access to one project host, as 0.1.7 stored it.
type hostAccess struct {
	Project     string `toml:"project"`
	Host        string `toml:"host"`
	User        string `toml:"user,omitempty"`
	InsecureTLS bool   `toml:"insecure_tls,omitempty"`
	Auth        Auth   `toml:"auth,omitempty"`
}

// secretsFile is the on-disk shape of secrets.toml.
type secretsFile struct {
	Secrets []hostSecret `toml:"secrets"`
}

// hostSecret is one project host's credentials, as 0.1.6 stored them.
type hostSecret struct {
	Project    string `toml:"project"`
	Host       string `toml:"host"`
	Password   string `toml:"password,omitempty"`
	Passphrase string `toml:"passphrase,omitempty"`
}

func decodeLegacyProjectConfig(root string) (*ProjectConfig, error) {
	path := legacyProjectConfig(root)
	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	cfg := &ProjectConfig{}
	if _, err := toml.DecodeFile(path, cfg); err != nil {
		return nil, err
	}
	return cfg, nil
}

func loadAccessFile() (*accessFile, error) {
	f := &accessFile{}
	if _, err := os.Stat(accessPath()); errors.Is(err, os.ErrNotExist) {
		return f, nil
	}
	if _, err := toml.DecodeFile(accessPath(), f); err != nil {
		return nil, err
	}
	return f, nil
}

func loadSecretsFile() (*secretsFile, error) {
	f := &secretsFile{}
	if _, err := os.Stat(secretsPath()); errors.Is(err, os.ErrNotExist) {
		return f, nil
	}
	if _, err := toml.DecodeFile(secretsPath(), f); err != nil {
		return nil, err
	}
	return f, nil
}

// hasLegacyFiles reports whether anything is still stored the old way for the
// project at root: a config in the project itself, or an entry in one of the
// two per-project stores.
func hasLegacyFiles(root string) bool {
	if root == "" {
		return false
	}
	if _, err := os.Stat(legacyProjectConfig(root)); err == nil {
		return true
	}
	access, aerr := loadAccessFile()
	secrets, serr := loadSecretsFile()
	if aerr != nil || serr != nil {
		return false
	}
	return storesHold(root, access, secrets)
}

// FindLegacyProjectRoot walks up from startDir looking for a .drift/config.toml
// and returns the directory holding it.
//
// It is the only thing left that treats a file in the project as a project
// marker, and it exists so an unregistered directory that still has one can be
// offered for registration and then migrated.
func FindLegacyProjectRoot(startDir string) (string, bool) {
	dir := startDir
	for {
		if _, err := os.Stat(legacyProjectConfig(dir)); err == nil {
			return dir, true
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", false
		}
		dir = parent
	}
}

// MigrationResult says what MigrateProjectToStore did, so the caller can tell
// the user about it.
type MigrationResult struct {
	Hosts       int      // hosts written into the project store
	ProjectFile bool     // a .drift/config.toml was read and removed
	Exposure    Exposure // whether git could reach that file
}

// Moved reports whether anything happened at all.
func (r MigrationResult) Moved() bool { return r.Hosts > 0 || r.ProjectFile }

// MigrateProjectToStore collects a project's configuration from the three
// legacy sources, writes it to <config.Dir()>/projects/<slug>.toml, and removes
// the sources. It reads from disk and shares no state with a running session,
// so it is safe in a goroutine, and it is idempotent: with nothing left in the
// old places it touches no file.
//
// Order matters. The store is written before anything is deleted, so an
// interruption costs a duplicate rather than the only copy.
func MigrateProjectToStore(root, slug string) (MigrationResult, error) {
	var res MigrationResult
	if root == "" || slug == "" {
		return res, nil
	}

	legacy, err := decodeLegacyProjectConfig(root)
	if err != nil {
		return res, fmt.Errorf("project config: %w", err)
	}
	access, err := loadAccessFile()
	if err != nil {
		return res, fmt.Errorf("access store: %w", err)
	}
	secrets, err := loadSecretsFile()
	if err != nil {
		return res, fmt.Errorf("secret store: %w", err)
	}

	if legacy == nil && !storesHold(root, access, secrets) {
		return res, nil
	}

	// Whether git could reach the project config has to be asked before it is
	// deleted: a committed credential stays in the repository's history.
	if legacy != nil {
		res.ProjectFile = true
		res.Exposure = ProjectConfigExposure(root)
	}

	store, err := loadProjectStore(slug)
	if err != nil {
		return res, fmt.Errorf("project store: %w", err)
	}
	target := ProjectConfig{}
	if store != nil {
		target = *store
	}

	if legacy != nil {
		if target.Defaults == (Defaults{}) {
			target.Defaults = legacy.Defaults
		}
		if len(target.Mappings) == 0 {
			target.Mappings = legacy.Mappings
		}
		for _, h := range legacy.Hosts {
			// A host already in the store is the migrated one and wins.
			if hostIndex(target.Hosts, h.Name) >= 0 {
				continue
			}
			target.Hosts = append(target.Hosts, h)
			res.Hosts++
		}
	}

	// Every host in the store gets what the old stores still hold for it, not
	// just the ones that came from the project config: a project whose
	// .drift/config.toml is already gone still has its credentials here.
	applied := make(map[string]bool, len(target.Hosts))
	for i, h := range target.Hosts {
		filled, used := applyLegacyAccess(h, root, access, secrets)
		target.Hosts[i] = filled
		if used {
			applied[h.Name] = true
		}
	}

	if err := writeProjectStore(slug, target); err != nil {
		return res, err
	}
	if err := removeLegacyProjectFile(root); err != nil {
		return res, err
	}
	// Only entries that were actually taken over are dropped. One naming a
	// host nobody knows stays put rather than being deleted unread.
	if err := pruneAccess(access, root, applied); err != nil {
		return res, err
	}
	return res, pruneSecrets(secrets, root, applied)
}

// applyLegacyAccess fills a host with the access and credentials the old stores
// hold for it, and reports whether it found an entry at all. What the host
// already carries wins: it came from a project config a person wrote by hand,
// or from a store that is already the newer one.
//
// Paths are compared cleaned. The old stores are keyed by absolute project
// path and the registry supplies the path here, so a trailing slash or a "."
// segment must not cost someone their credentials.
func applyLegacyAccess(h Host, root string, access *accessFile, secrets *secretsFile) (Host, bool) {
	found := false
	for _, a := range access.Access {
		if !samePath(a.Project, root) || a.Host != h.Name {
			continue
		}
		found = true
		if h.User == "" {
			h.User = a.User
		}
		if !h.InsecureTLS {
			h.InsecureTLS = a.InsecureTLS
		}
		if h.Auth.Type == "" {
			h.Auth.Type = a.Auth.Type
		}
		if h.Auth.KeyFile == "" {
			h.Auth.KeyFile = a.Auth.KeyFile
		}
		if h.Auth.Password == "" {
			h.Auth.Password = a.Auth.Password
		}
		if h.Auth.Passphrase == "" {
			h.Auth.Passphrase = a.Auth.Passphrase
		}
	}
	for _, s := range secrets.Secrets {
		if !samePath(s.Project, root) || s.Host != h.Name {
			continue
		}
		found = true
		if h.Auth.Password == "" {
			h.Auth.Password = s.Password
		}
		if h.Auth.Passphrase == "" {
			h.Auth.Passphrase = s.Passphrase
		}
	}
	return h, found
}

// storesHold reports whether either old store still has an entry for root.
func storesHold(root string, access *accessFile, secrets *secretsFile) bool {
	for _, a := range access.Access {
		if samePath(a.Project, root) {
			return true
		}
	}
	for _, s := range secrets.Secrets {
		if samePath(s.Project, root) {
			return true
		}
	}
	return false
}

func samePath(a, b string) bool {
	return filepath.Clean(a) == filepath.Clean(b)
}

func hostIndex(hosts []Host, name string) int {
	for i, h := range hosts {
		if h.Name == name {
			return i
		}
	}
	return -1
}

// removeLegacyProjectFile deletes <root>/.drift/config.toml, and .drift/ with
// it when nothing but drift's own .gitignore is left there. A .drift/ holding
// anything else is left alone: it is not drift's to clear out.
func removeLegacyProjectFile(root string) error {
	path := legacyProjectConfig(root)
	if err := os.Remove(path); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		return err
	}

	dir := filepath.Dir(path)
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	for _, e := range entries {
		if e.Name() != ".gitignore" {
			return nil
		}
		data, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil || string(data) != projectGitignore {
			return nil
		}
	}
	return os.RemoveAll(dir)
}

// pruneAccess drops the entries of this project that were taken over, and
// removes the file once it holds nothing.
func pruneAccess(f *accessFile, root string, applied map[string]bool) error {
	kept := make([]hostAccess, 0, len(f.Access))
	for _, a := range f.Access {
		if samePath(a.Project, root) && applied[a.Host] {
			continue
		}
		kept = append(kept, a)
	}
	if len(kept) == len(f.Access) {
		return nil
	}
	if len(kept) == 0 {
		return removeIfExists(accessPath())
	}
	return writeToml(accessPath(), accessFile{Access: kept})
}

// pruneSecrets does the same for secrets.toml.
func pruneSecrets(f *secretsFile, root string, applied map[string]bool) error {
	kept := make([]hostSecret, 0, len(f.Secrets))
	for _, s := range f.Secrets {
		if samePath(s.Project, root) && applied[s.Host] {
			continue
		}
		kept = append(kept, s)
	}
	if len(kept) == len(f.Secrets) {
		return nil
	}
	if len(kept) == 0 {
		return removeIfExists(secretsPath())
	}
	return writeToml(secretsPath(), secretsFile{Secrets: kept})
}

func removeIfExists(path string) error {
	if err := os.Remove(path); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return nil
}
