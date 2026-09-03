package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// readProjectConfig returns the raw project config as written to disk.
func readProjectConfig(t *testing.T, projectRoot string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(projectRoot, ".drift", "config.toml"))
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}
	return string(data)
}

func TestSaveProjectHostKeepsTheCredentialOutOfTheProject(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{
		Name:     "prod",
		Hostname: "example.com",
		Port:     22,
		RootPath: "/var/www",
		Auth:     Auth{Type: "password", Password: "hunter2"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	written := readProjectConfig(t, projectRoot)
	if strings.Contains(written, "hunter2") {
		t.Fatalf("the credential was written into the project:\n%s", written)
	}
	if !strings.Contains(written, `hostname = "example.com"`) {
		t.Fatalf("the project config lost the host itself:\n%s", written)
	}
	// The running session still needs the credential to connect.
	if got := cfg.Hosts["prod"].Auth.Password; got != "hunter2" {
		t.Fatalf("in-memory password = %q, want it kept", got)
	}

	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := loaded.Hosts["prod"].Auth.Password; got != "hunter2" {
		t.Fatalf("password did not come back from the store: %q", got)
	}
	if len(loaded.ProjectSecretsInFile) != 0 {
		t.Fatalf("ProjectSecretsInFile = %v, want empty for a stored credential", loaded.ProjectSecretsInFile)
	}
}

func TestSaveProjectHostLeavesEnvReferencesInTheConfig(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{
		Name: "prod",
		Auth: Auth{Type: "password", Password: "$DEPLOY_PW"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	if written := readProjectConfig(t, projectRoot); !strings.Contains(written, "$DEPLOY_PW") {
		t.Fatalf("the $ENV reference was moved out of the config:\n%s", written)
	}
	if _, err := os.Stat(secretsPath()); err == nil {
		t.Fatal("an $ENV reference created a secret store entry")
	}
}

func TestSaveProjectHostWritesEachHostOnce(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	for _, h := range []Host{
		{Name: "prod", Hostname: "example.com", Auth: Auth{Type: "password", Password: "hunter2"}},
		{Name: "staging", Hostname: "staging.example.com", Auth: Auth{Type: "password", Password: "$STAGING_PW"}},
	} {
		if err := SaveProjectHost(cfg, h, ""); err != nil {
			t.Fatalf("SaveProjectHost returned error: %v", err)
		}
	}

	written := readProjectConfig(t, projectRoot)
	if got := strings.Count(written, `name = "staging"`); got != 1 {
		t.Fatalf("staging appears %d times in the config, want 1:\n%s", got, written)
	}
	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if len(loaded.ProjectHosts) != 2 {
		t.Fatalf("loaded %d project hosts, want 2", len(loaded.ProjectHosts))
	}
}

func TestSaveProjectHostRenameMovesTheStoredCredential(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{Name: "prod", Auth: Auth{Type: "password", Password: "hunter2"}}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	if err := SaveProjectHost(cfg, Host{Name: "production", Auth: Auth{Type: "password", Password: "hunter2"}}, "prod"); err != nil {
		t.Fatalf("SaveProjectHost (rename) returned error: %v", err)
	}

	store, err := loadSecrets()
	if err != nil {
		t.Fatalf("loadSecrets returned error: %v", err)
	}
	if len(store.Secrets) != 1 {
		t.Fatalf("store holds %d entries after a rename, want 1: %+v", len(store.Secrets), store.Secrets)
	}
	if store.Secrets[0].Host != "production" {
		t.Fatalf("stored entry is keyed by %q, want the new name", store.Secrets[0].Host)
	}
}

func TestDeleteProjectHostDropsTheStoredCredential(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{Name: "prod", Auth: Auth{Type: "password", Password: "hunter2"}}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	if err := SaveProjectHost(cfg, Host{Name: "staging", Auth: Auth{Type: "password", Password: "letmein"}}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	if err := DeleteProjectHost(cfg, "prod"); err != nil {
		t.Fatalf("DeleteProjectHost returned error: %v", err)
	}

	store, err := loadSecrets()
	if err != nil {
		t.Fatalf("loadSecrets returned error: %v", err)
	}
	if len(store.Secrets) != 1 || store.Secrets[0].Host != "staging" {
		t.Fatalf("store after delete = %+v, want only staging", store.Secrets)
	}
}

func TestSecretsOfTwoProjectsDoNotCollide(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	rootA, rootB := t.TempDir(), t.TempDir()

	for root, pw := range map[string]string{rootA: "secret-a", rootB: "secret-b"} {
		cfg := &MergedConfig{ProjectRoot: root}
		if err := SaveProjectHost(cfg, Host{Name: "prod", Auth: Auth{Type: "password", Password: pw}}, ""); err != nil {
			t.Fatalf("SaveProjectHost returned error: %v", err)
		}
	}

	for root, want := range map[string]string{rootA: "secret-a", rootB: "secret-b"} {
		loaded, err := Load(root)
		if err != nil {
			t.Fatalf("Load returned error: %v", err)
		}
		if got := loaded.Hosts["prod"].Auth.Password; got != want {
			t.Fatalf("project %s got password %q, want %q", root, got, want)
		}
	}
}

func TestMigrateProjectSecretsMovesLiteralsAndIsIdempotent(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	if err := os.MkdirAll(filepath.Join(projectRoot, ".drift"), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	// A config as drift wrote it before the secret store existed.
	legacy := `[[hosts]]
name = "prod"
hostname = "example.com"
root_path = "/var/www"

  [hosts.auth]
  type = "keyfile"
  key_file = "~/.ssh/id_ed25519"
  passphrase = "s3cret"

[[hosts]]
name = "staging"
hostname = "staging.example.com"

  [hosts.auth]
  type = "password"
  password = "$STAGING_PW"
`
	if err := os.WriteFile(filepath.Join(projectRoot, ".drift", "config.toml"), []byte(legacy), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}

	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if want := []string{"prod"}; len(loaded.ProjectSecretsInFile) != 1 || loaded.ProjectSecretsInFile[0] != want[0] {
		t.Fatalf("ProjectSecretsInFile = %v, want %v", loaded.ProjectSecretsInFile, want)
	}

	n, err := MigrateProjectSecrets(projectRoot)
	if err != nil {
		t.Fatalf("MigrateProjectSecrets returned error: %v", err)
	}
	if n != 1 {
		t.Fatalf("migrated %d hosts, want 1", n)
	}

	written := readProjectConfig(t, projectRoot)
	if strings.Contains(written, "s3cret") {
		t.Fatalf("the passphrase stayed in the project:\n%s", written)
	}
	if !strings.Contains(written, "$STAGING_PW") {
		t.Fatalf("the $ENV reference was moved out:\n%s", written)
	}
	if !strings.Contains(written, `key_file = "~/.ssh/id_ed25519"`) {
		t.Fatalf("migration dropped a non-secret field:\n%s", written)
	}

	// The credential is still there after a reload, and a second run is a no-op.
	reloaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := reloaded.Hosts["prod"].Auth.Passphrase; got != "s3cret" {
		t.Fatalf("passphrase after migration = %q, want it in the store", got)
	}
	if len(reloaded.ProjectSecretsInFile) != 0 {
		t.Fatalf("ProjectSecretsInFile = %v, want empty after migration", reloaded.ProjectSecretsInFile)
	}
	if n, err := MigrateProjectSecrets(projectRoot); err != nil || n != 0 {
		t.Fatalf("second migration = (%d, %v), want (0, nil)", n, err)
	}
}

func TestMigrateProjectSecretsWithoutAConfigDoesNothing(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	if n, err := MigrateProjectSecrets(t.TempDir()); err != nil || n != 0 {
		t.Fatalf("migration without a config = (%d, %v), want (0, nil)", n, err)
	}
	if n, err := MigrateProjectSecrets(""); err != nil || n != 0 {
		t.Fatalf("migration without a root = (%d, %v), want (0, nil)", n, err)
	}
}

func TestSecretStoreFilePermissions(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Auth: Auth{Type: "password", Password: "hunter2"}}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	info, err := os.Stat(secretsPath())
	if err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
	if perm := info.Mode().Perm(); perm != 0o600 {
		t.Fatalf("secrets.toml permissions = %o, want 600", perm)
	}
	dir, err := os.Stat(filepath.Dir(secretsPath()))
	if err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
	if perm := dir.Mode().Perm(); perm != 0o700 {
		t.Fatalf("config dir permissions = %o, want 700", perm)
	}
}
