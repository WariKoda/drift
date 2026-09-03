package config

import (
	"errors"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

// A project host is described by two layers with different owners.
//
// The project config (.drift/config.toml) describes the environment: hostname,
// port, root path, protocol, mappings. It is a statement about the repository,
// it is the same for everyone who clones it, and it is meant to be committed.
//
// The access layer (<config.Dir()>/access.toml) describes how one person on one
// machine reaches that environment: user, auth, insecure TLS, and the
// credentials themselves. It never leaves the machine.
//
// Reading merges the two, with the project config winning where it carries a
// value of its own, so a hand-written config keeps working. Writing only ever
// puts environment fields into the project.
//
// Global hosts are unaffected: ~/.config/drift/config.toml is already outside
// every repository, so its hosts keep their access fields.

// accessFile is the on-disk shape of <config.Dir()>/access.toml.
type accessFile struct {
	Access []hostAccess `toml:"access"`
}

// hostAccess is one person's access to one project host.
type hostAccess struct {
	Project     string `toml:"project"` // absolute project root
	Host        string `toml:"host"`    // Host.Name
	User        string `toml:"user,omitempty"`
	InsecureTLS bool   `toml:"insecure_tls,omitempty"`
	Auth        Auth   `toml:"auth,omitempty"`
}

func (a hostAccess) empty() bool {
	return a.User == "" && !a.InsecureTLS && a.Auth == Auth{}
}

func accessPath() string {
	return filepath.Join(Dir(), "access.toml")
}

// legacySecretsPath is the 0.1.6-alpha store, which held credentials only. It
// is folded into access.toml and removed by MigrateProjectSecrets.
func legacySecretsPath() string {
	return filepath.Join(Dir(), "secrets.toml")
}

// loadAccess reads the access store, returning an empty store if the file does
// not exist yet.
func loadAccess() (*accessFile, error) {
	f := &accessFile{}
	if _, err := os.Stat(accessPath()); errors.Is(err, os.ErrNotExist) {
		return f, nil
	}
	if _, err := toml.DecodeFile(accessPath(), f); err != nil {
		return nil, err
	}
	return f, nil
}

// writeAccess persists the store. The file is 0600 in a 0700 directory: it is
// the one place drift keeps credentials verbatim.
func writeAccess(f *accessFile) error {
	if err := os.MkdirAll(filepath.Dir(accessPath()), 0o700); err != nil {
		return err
	}
	return writeToml(accessPath(), f)
}

// find returns the index of the entry for projectRoot/hostName, or -1.
func (f *accessFile) find(projectRoot, hostName string) int {
	for i, a := range f.Access {
		if a.Project == projectRoot && a.Host == hostName {
			return i
		}
	}
	return -1
}

// put stores a, replacing any entry for the same project and host. An empty
// entry removes the stored one instead, so the store never accumulates blanks.
func (f *accessFile) put(a hostAccess) {
	i := f.find(a.Project, a.Host)
	if a.empty() {
		if i >= 0 {
			f.Access = append(f.Access[:i], f.Access[i+1:]...)
		}
		return
	}
	if i >= 0 {
		f.Access[i] = a
		return
	}
	f.Access = append(f.Access, a)
}

// remove drops the entry for projectRoot/hostName if there is one.
func (f *accessFile) remove(projectRoot, hostName string) {
	f.put(hostAccess{Project: projectRoot, Host: hostName})
}

// splitAccess separates a host into what belongs in the project config and what
// belongs in the access store. It is the write path: everything about how this
// machine authenticates goes to the store, including an $ENV reference, because
// the mechanism ("I use a key, you use a password") is per person even when the
// value is not a secret.
func splitAccess(h Host, projectRoot string) (Host, hostAccess) {
	a := hostAccess{
		Project:     projectRoot,
		Host:        h.Name,
		User:        h.User,
		InsecureTLS: h.InsecureTLS,
		Auth:        h.Auth,
	}
	h.User = ""
	h.InsecureTLS = false
	h.Auth = Auth{}
	return h, a
}

// splitSecret separates only the credentials that must not stay in a file
// inside the project. It is the migration path: unlike splitAccess it leaves
// user, auth type, key file and $ENV references where they are, because
// rewriting a config that a team shares should remove leaks and nothing else.
func splitSecret(h Host, projectRoot string) (Host, hostAccess) {
	a := hostAccess{Project: projectRoot, Host: h.Name}
	if isLiteralSecret(h.Auth.Password) {
		a.Auth.Password = h.Auth.Password
		h.Auth.Password = ""
	}
	if isLiteralSecret(h.Auth.Passphrase) {
		a.Auth.Passphrase = h.Auth.Passphrase
		h.Auth.Passphrase = ""
	}
	return h, a
}

// applyAccess fills in the access fields the project config does not carry
// itself. A value present in the config wins: a hand-written file, or one a
// team maintains deliberately, keeps working.
func applyAccess(h Host, a hostAccess) Host {
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
	return h
}

// applyProjectAccess fills in the stored access of every project host.
func applyProjectAccess(hosts []Host, projectRoot string) ([]Host, error) {
	if len(hosts) == 0 {
		return hosts, nil
	}
	store, err := loadAccess()
	if err != nil {
		return nil, err
	}
	if len(store.Access) == 0 {
		return hosts, nil
	}
	for i, h := range hosts {
		if found := store.find(projectRoot, h.Name); found >= 0 {
			hosts[i] = applyAccess(h, store.Access[found])
		}
	}
	return hosts, nil
}

// storeAccess writes one host's access to the store. oldName, when set and
// different, is the name the host had before: its entry is dropped so a rename
// does not leave access behind under the old name.
func storeAccess(a hostAccess, oldName string) error {
	store, err := loadAccess()
	if err != nil {
		return err
	}

	renamed := oldName != "" && oldName != a.Host && store.find(a.Project, oldName) >= 0
	i := store.find(a.Project, a.Host)
	switch {
	case renamed:
	case a.empty() && i < 0:
		// Nothing to store and nothing stored: leave the file alone rather
		// than creating an empty one for a host that has no access of its own.
		return nil
	case i >= 0 && store.Access[i] == a:
		return nil
	}

	if renamed {
		store.remove(a.Project, oldName)
	}
	store.put(a)
	return writeAccess(store)
}

// deleteAccess removes one host's access from the store.
func deleteAccess(projectRoot, hostName string) error {
	store, err := loadAccess()
	if err != nil {
		return err
	}
	if store.find(projectRoot, hostName) < 0 {
		return nil
	}
	store.remove(projectRoot, hostName)
	return writeAccess(store)
}

// MigrateProjectSecrets moves credentials that <projectRoot>/.drift/config.toml
// still carries into the access store, writes the config back without them, and
// folds a 0.1.6-alpha secrets.toml into access.toml. It reports how many hosts
// it moved a credential for; 0 means no project config was rewritten, which
// says nothing about whether the fold happened.
//
// The fold is not tied to projectRoot: secrets.toml holds every project's
// credentials, so it runs even when this project has nothing left to migrate
// and even when projectRoot is empty. A project whose config was already clean
// would otherwise keep its credentials in a file nothing reads any more.
//
// It deliberately moves leaks only. User, auth type, key file and $ENV
// references stay in the project config until that host is edited, because a
// project config can be a file a team maintains, and silently deleting lines
// from it would be worse than leaving them: the next person to pull would lose
// a value drift never stored for them.
//
// It reads from disk rather than taking a loaded MergedConfig, so it shares no
// state with a running session and is safe in a goroutine. Migration is
// idempotent: with nothing left to move, no file is touched.
func MigrateProjectSecrets(projectRoot string) (int, error) {
	store, err := loadAccess()
	if err != nil {
		return 0, err
	}
	foldedLegacy, err := foldLegacySecrets(store)
	if err != nil {
		return 0, err
	}

	var pc *ProjectConfig
	if projectRoot != "" {
		if pc, err = decodeProjectConfig(projectRoot); err != nil {
			return 0, err
		}
	}

	migrated := 0
	var hosts []Host
	if pc != nil {
		hosts = make([]Host, len(pc.Hosts))
		for i, h := range pc.Hosts {
			stripped, secret := splitSecret(h, projectRoot)
			hosts[i] = stripped
			if secret.empty() {
				continue
			}
			// Merge into whatever access is already stored: the credential is
			// one field of it, not a replacement for the whole entry.
			if found := store.find(projectRoot, h.Name); found >= 0 {
				secret = mergeSecretInto(store.Access[found], secret)
			}
			store.put(secret)
			migrated++
		}
	}

	if migrated == 0 && !foldedLegacy {
		return 0, nil
	}

	// The store is written first: a crash in between costs a duplicated
	// credential, the other order would lose it.
	if err := writeAccess(store); err != nil {
		return 0, err
	}
	if migrated > 0 {
		pc.Hosts = hosts
		if err := writeProject(*pc, projectRoot); err != nil {
			return 0, err
		}
	}
	if foldedLegacy {
		if err := os.Remove(legacySecretsPath()); err != nil && !errors.Is(err, os.ErrNotExist) {
			return migrated, err
		}
	}
	return migrated, nil
}

// mergeSecretInto puts a migrated credential into an existing access entry
// without discarding the rest of it.
func mergeSecretInto(existing, secret hostAccess) hostAccess {
	if secret.Auth.Password != "" {
		existing.Auth.Password = secret.Auth.Password
	}
	if secret.Auth.Passphrase != "" {
		existing.Auth.Passphrase = secret.Auth.Passphrase
	}
	return existing
}

// foldLegacySecrets copies a 0.1.6-alpha secrets.toml into the access store.
// It reports whether there was one; the caller removes the file after the
// access store is safely written.
func foldLegacySecrets(store *accessFile) (bool, error) {
	if _, err := os.Stat(legacySecretsPath()); errors.Is(err, os.ErrNotExist) {
		return false, nil
	} else if err != nil {
		return false, err
	}

	var legacy struct {
		Secrets []struct {
			Project    string `toml:"project"`
			Host       string `toml:"host"`
			Password   string `toml:"password"`
			Passphrase string `toml:"passphrase"`
		} `toml:"secrets"`
	}
	if _, err := toml.DecodeFile(legacySecretsPath(), &legacy); err != nil {
		return false, err
	}

	for _, s := range legacy.Secrets {
		a := hostAccess{Project: s.Project, Host: s.Host}
		if found := store.find(s.Project, s.Host); found >= 0 {
			a = store.Access[found]
		}
		// An entry already in access.toml is the newer one and wins.
		if a.Auth.Password == "" {
			a.Auth.Password = s.Password
		}
		if a.Auth.Passphrase == "" {
			a.Auth.Passphrase = s.Passphrase
		}
		store.put(a)
	}
	return true, nil
}

// hasLegacySecrets reports whether a 0.1.6-alpha secrets.toml is still around.
// Nothing reads it any more, so its credentials are invisible until
// MigrateProjectSecrets folds them in.
func hasLegacySecrets() bool {
	_, err := os.Stat(legacySecretsPath())
	return err == nil
}

// AccessPathForDisplay is the access store's path, for messages that tell the
// user where their credentials went.
func AccessPathForDisplay() string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return accessPath()
	}
	if rel := strings.TrimPrefix(accessPath(), home); rel != accessPath() {
		return "~" + rel
	}
	return accessPath()
}
