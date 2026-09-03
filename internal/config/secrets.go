package config

import (
	"errors"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

// Credentials are kept out of .drift/config.toml so that the project config —
// hostnames, root paths, mappings — can be committed and shared. They live in
// <config.Dir()>/secrets.toml instead, keyed by project root and host name.
//
// Global hosts are unaffected: ~/.config/drift/config.toml is already outside
// every repository.

// secretsFile is the on-disk shape of <config.Dir()>/secrets.toml.
type secretsFile struct {
	Secrets []hostSecret `toml:"secrets"`
}

// hostSecret holds one project host's credentials.
type hostSecret struct {
	Project    string `toml:"project"` // absolute project root
	Host       string `toml:"host"`    // Host.Name
	Password   string `toml:"password,omitempty"`
	Passphrase string `toml:"passphrase,omitempty"`
}

func (s hostSecret) empty() bool {
	return s.Password == "" && s.Passphrase == ""
}

func secretsPath() string {
	return filepath.Join(Dir(), "secrets.toml")
}

// loadSecrets reads the secret store, returning an empty store if the file
// does not exist yet.
func loadSecrets() (*secretsFile, error) {
	f := &secretsFile{}
	path := secretsPath()
	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) {
		return f, nil
	}
	if _, err := toml.DecodeFile(path, f); err != nil {
		return nil, err
	}
	return f, nil
}

// writeSecrets persists the store. The file is 0600 in a 0700 directory: it is
// the one place drift keeps credentials verbatim.
func writeSecrets(f *secretsFile) error {
	path := secretsPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	return writeToml(path, f)
}

// find returns the index of the entry for projectRoot/hostName, or -1.
func (f *secretsFile) find(projectRoot, hostName string) int {
	for i, s := range f.Secrets {
		if s.Project == projectRoot && s.Host == hostName {
			return i
		}
	}
	return -1
}

// put stores s, replacing any entry for the same project and host. An empty
// secret removes the entry instead, so the store never accumulates blanks.
func (f *secretsFile) put(s hostSecret) {
	i := f.find(s.Project, s.Host)
	if s.empty() {
		if i >= 0 {
			f.Secrets = append(f.Secrets[:i], f.Secrets[i+1:]...)
		}
		return
	}
	if i >= 0 {
		f.Secrets[i] = s
		return
	}
	f.Secrets = append(f.Secrets, s)
}

// remove drops the entry for projectRoot/hostName if there is one.
func (f *secretsFile) remove(projectRoot, hostName string) {
	f.put(hostSecret{Project: projectRoot, Host: hostName})
}

// splitSecret separates a host's literal credentials from the rest of it. The
// returned Host is what belongs in the project config; the hostSecret is what
// belongs in the store. $ENV references stay in the config — they are already
// safe to commit and expanding them is the connect code's job.
func splitSecret(h Host, projectRoot string) (Host, hostSecret) {
	s := hostSecret{Project: projectRoot, Host: h.Name}
	if isLiteralSecret(h.Auth.Password) {
		s.Password = h.Auth.Password
		h.Auth.Password = ""
	}
	if isLiteralSecret(h.Auth.Passphrase) {
		s.Passphrase = h.Auth.Passphrase
		h.Auth.Passphrase = ""
	}
	return h, s
}

// applySecret fills in credentials the config does not carry itself. A value
// left in the config wins, so a hand-written file keeps working.
func applySecret(h Host, s hostSecret) Host {
	if h.Auth.Password == "" {
		h.Auth.Password = s.Password
	}
	if h.Auth.Passphrase == "" {
		h.Auth.Passphrase = s.Passphrase
	}
	return h
}

// applyProjectSecrets fills in the stored credentials of every project host.
func applyProjectSecrets(hosts []Host, projectRoot string) ([]Host, error) {
	if len(hosts) == 0 {
		return hosts, nil
	}
	store, err := loadSecrets()
	if err != nil {
		return nil, err
	}
	if len(store.Secrets) == 0 {
		return hosts, nil
	}
	for i, h := range hosts {
		if found := store.find(projectRoot, h.Name); found >= 0 {
			hosts[i] = applySecret(h, store.Secrets[found])
		}
	}
	return hosts, nil
}

// storeSecret writes one host's credentials to the store. oldName, when set
// and different, is the name the host had before: its entry is dropped so a
// rename does not leave a secret behind under the old name.
func storeSecret(s hostSecret, oldName string) error {
	store, err := loadSecrets()
	if err != nil {
		return err
	}

	renamed := oldName != "" && oldName != s.Host && store.find(s.Project, oldName) >= 0
	i := store.find(s.Project, s.Host)
	switch {
	case renamed:
	case s.empty() && i < 0:
		// Nothing to store and nothing stored: leave the file alone rather
		// than creating an empty one for a host that has no credential.
		return nil
	case i >= 0 && store.Secrets[i] == s:
		return nil
	}

	if renamed {
		store.remove(s.Project, oldName)
	}
	store.put(s)
	return writeSecrets(store)
}

// deleteSecret removes one host's credentials from the store.
func deleteSecret(projectRoot, hostName string) error {
	store, err := loadSecrets()
	if err != nil {
		return err
	}
	if store.find(projectRoot, hostName) < 0 {
		return nil
	}
	store.remove(projectRoot, hostName)
	return writeSecrets(store)
}

// MigrateProjectSecrets moves literal credentials out of
// <projectRoot>/.drift/config.toml into the secret store and writes the config
// back without them. It reports how many hosts were migrated; 0 means there
// was nothing to do and no file was touched.
//
// It reads the config from disk rather than taking a loaded MergedConfig, so
// it shares no state with a running session and is safe to call from a
// goroutine. Migration is idempotent: a config that carries no literal
// credential is left alone.
func MigrateProjectSecrets(projectRoot string) (int, error) {
	if projectRoot == "" {
		return 0, nil
	}
	pc, err := decodeProjectConfig(projectRoot)
	if err != nil || pc == nil {
		return 0, err
	}

	store, err := loadSecrets()
	if err != nil {
		return 0, err
	}

	migrated := 0
	hosts := make([]Host, len(pc.Hosts))
	for i, h := range pc.Hosts {
		stripped, secret := splitSecret(h, projectRoot)
		hosts[i] = stripped
		if secret.empty() {
			continue
		}
		store.put(secret)
		migrated++
	}
	if migrated == 0 {
		return 0, nil
	}

	// The store is written first: a crash in between costs a duplicated
	// secret, the other order would lose it.
	if err := writeSecrets(store); err != nil {
		return 0, err
	}
	pc.Hosts = hosts
	if err := writeProject(*pc, projectRoot); err != nil {
		return 0, err
	}
	return migrated, nil
}

// SecretsPathForDisplay is the secret store's path, for messages that tell the
// user where their credentials went.
func SecretsPathForDisplay() string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return secretsPath()
	}
	if rel := strings.TrimPrefix(secretsPath(), home); rel != secretsPath() {
		return "~" + rel
	}
	return secretsPath()
}
