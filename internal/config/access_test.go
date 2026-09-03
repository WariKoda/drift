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

// writeProjectConfig puts a hand-written project config in place.
func writeProjectConfig(t *testing.T, projectRoot, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(projectRoot, ".drift"), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	if err := os.WriteFile(filepath.Join(projectRoot, ".drift", "config.toml"), []byte(body), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
}

func TestSaveProjectHostKeepsAccessOutOfTheProject(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{
		Name:        "prod",
		Hostname:    "example.com",
		Port:        21,
		User:        "webuser",
		RootPath:    "/var/www",
		Protocol:    "ftps",
		InsecureTLS: true,
		Auth:        Auth{Type: "password", Password: "hunter2"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	written := readProjectConfig(t, projectRoot)
	for _, unwanted := range []string{"hunter2", "webuser", "insecure_tls", "[hosts.auth]"} {
		if strings.Contains(written, unwanted) {
			t.Fatalf("access field %q was written into the project:\n%s", unwanted, written)
		}
	}
	for _, wanted := range []string{`hostname = "example.com"`, `port = 21`, `root_path = "/var/www"`, `protocol = "ftps"`} {
		if !strings.Contains(written, wanted) {
			t.Fatalf("the project config lost the environment field %q:\n%s", wanted, written)
		}
	}

	// The running session still needs the access fields to connect.
	if got := cfg.Hosts["prod"]; got.Auth.Password != "hunter2" || got.User != "webuser" || !got.InsecureTLS {
		t.Fatalf("in-memory host lost its access fields: %+v", got)
	}

	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	got := loaded.Hosts["prod"]
	if got.Auth.Password != "hunter2" || got.User != "webuser" || !got.InsecureTLS || got.Auth.Type != "password" {
		t.Fatalf("access did not come back from the store: %+v", got)
	}
	if len(loaded.ProjectSecretsInFile) != 0 {
		t.Fatalf("ProjectSecretsInFile = %v, want empty", loaded.ProjectSecretsInFile)
	}
}

func TestProjectConfigWinsOverTheAccessStore(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()

	cfg := &MergedConfig{ProjectRoot: projectRoot}
	if err := SaveProjectHost(cfg, Host{
		Name: "prod", Hostname: "example.com",
		User: "stored-user",
		Auth: Auth{Type: "password", Password: "stored-pw"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	// A team maintains these two lines by hand.
	writeProjectConfig(t, projectRoot, `[[hosts]]
name = "prod"
hostname = "example.com"
user = "deploy"

  [hosts.auth]
  password = "$DEPLOY_PW"
`)

	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	got := loaded.Hosts["prod"]
	if got.User != "deploy" {
		t.Fatalf("user = %q, want the project config's value", got.User)
	}
	if got.Auth.Password != "$DEPLOY_PW" {
		t.Fatalf("password = %q, want the project config's value", got.Auth.Password)
	}
	// Fields the config does not mention still come from the store.
	if got.Auth.Type != "password" {
		t.Fatalf("auth type = %q, want it filled in from the store", got.Auth.Type)
	}
}

func TestSavingOneHostLeavesTheOthersUntouchedInTheFile(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()

	// staging omits port and user on purpose: they come from [defaults], and
	// a save must not bake them into the host's own record.
	writeProjectConfig(t, projectRoot, `[defaults]
port = 2222
user = "deploy"

[[hosts]]
name = "staging"
hostname = "staging.example.com"
root_path = "/srv/app"

  [hosts.auth]
  type = "agent"
`)

	cfg, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := cfg.Hosts["staging"]; got.Port != 2222 || got.User != "deploy" {
		t.Fatalf("defaults were not applied in memory: %+v", got)
	}

	if err := SaveProjectHost(cfg, Host{
		Name: "prod", Hostname: "example.com", Port: 22,
		Auth: Auth{Type: "password", Password: "hunter2"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	written := readProjectConfig(t, projectRoot)
	if strings.Contains(written, `user = "deploy"`) && strings.Count(written, `user = "deploy"`) > 1 {
		t.Fatalf("the default user was baked into a host record:\n%s", written)
	}
	if strings.Contains(written, "port = 2222") && strings.Count(written, "port = 2222") > 1 {
		t.Fatalf("the default port was baked into a host record:\n%s", written)
	}
	if !strings.Contains(written, `type = "agent"`) {
		t.Fatalf("staging lost its auth type, which only the file had:\n%s", written)
	}

	reloaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := reloaded.Hosts["staging"]; got.Port != 2222 || got.User != "deploy" || got.Auth.Type != "agent" {
		t.Fatalf("staging changed across the save: %+v", got)
	}
}

func TestSaveWithoutAProjectFileStripsAccessFromTheHostsItSeeds(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()

	// No file on disk yet, but the session already knows a host — its access
	// belongs in the store, not in the file that is about to be created.
	cfg := &MergedConfig{
		ProjectRoot:  projectRoot,
		ProjectHosts: []Host{{Name: "staging", Hostname: "staging.example.com", User: "webuser"}},
		Mappings:     []Mapping{{Local: "src", Remote: "html"}},
	}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Hostname: "example.com"}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	written := readProjectConfig(t, projectRoot)
	if strings.Contains(written, "webuser") {
		t.Fatalf("an access field was seeded into the new project config:\n%s", written)
	}
	if !strings.Contains(written, `name = "staging"`) {
		t.Fatalf("the known host was dropped from the new project config:\n%s", written)
	}
	if !strings.Contains(written, `local = "src"`) {
		t.Fatalf("the project mappings were dropped:\n%s", written)
	}
}

func TestSaveProjectHostWritesEachHostOnce(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	for _, h := range []Host{
		{Name: "prod", Hostname: "example.com", Auth: Auth{Type: "password", Password: "hunter2"}},
		{Name: "staging", Hostname: "staging.example.com", Auth: Auth{Type: "agent"}},
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

func TestSaveProjectHostRenameMovesTheStoredAccess(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{Name: "prod", User: "webuser"}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	if err := SaveProjectHost(cfg, Host{Name: "production", User: "webuser"}, "prod"); err != nil {
		t.Fatalf("SaveProjectHost (rename) returned error: %v", err)
	}

	store, err := loadAccess()
	if err != nil {
		t.Fatalf("loadAccess returned error: %v", err)
	}
	if len(store.Access) != 1 {
		t.Fatalf("store holds %d entries after a rename, want 1: %+v", len(store.Access), store.Access)
	}
	if store.Access[0].Host != "production" {
		t.Fatalf("stored entry is keyed by %q, want the new name", store.Access[0].Host)
	}
	if got := strings.Count(readProjectConfig(t, projectRoot), "[[hosts]]"); got != 1 {
		t.Fatalf("the project config holds %d hosts after a rename, want 1", got)
	}
}

func TestDeleteProjectHostDropsTheStoredAccess(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	for _, h := range []Host{
		{Name: "prod", User: "root", Auth: Auth{Type: "password", Password: "hunter2"}},
		{Name: "staging", User: "webuser", Auth: Auth{Type: "password", Password: "letmein"}},
	} {
		if err := SaveProjectHost(cfg, h, ""); err != nil {
			t.Fatalf("SaveProjectHost returned error: %v", err)
		}
	}
	if err := DeleteProjectHost(cfg, "prod"); err != nil {
		t.Fatalf("DeleteProjectHost returned error: %v", err)
	}

	store, err := loadAccess()
	if err != nil {
		t.Fatalf("loadAccess returned error: %v", err)
	}
	if len(store.Access) != 1 || store.Access[0].Host != "staging" {
		t.Fatalf("store after delete = %+v, want only staging", store.Access)
	}
	if strings.Contains(readProjectConfig(t, projectRoot), `name = "prod"`) {
		t.Fatal("the deleted host is still in the project config")
	}
}

func TestAccessOfTwoProjectsDoesNotCollide(t *testing.T) {
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

// legacyProjectConfig is a config as drift wrote it before 0.1.6-alpha: the
// passphrase is a leak, the rest is not.
const legacyProjectConfig = `[[hosts]]
name = "prod"
hostname = "example.com"
user = "deploy"
root_path = "/var/www"

  [hosts.auth]
  type = "keyfile"
  key_file = "~/.ssh/id_ed25519"
  passphrase = "s3cret"

[[hosts]]
name = "staging"
hostname = "staging.example.com"
user = "webuser"

  [hosts.auth]
  type = "password"
  password = "$STAGING_PW"
`

func TestMigrateMovesLeaksAndLeavesTheRestInPlace(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	writeProjectConfig(t, projectRoot, legacyProjectConfig)

	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if len(loaded.ProjectSecretsInFile) != 1 || loaded.ProjectSecretsInFile[0] != "prod" {
		t.Fatalf("ProjectSecretsInFile = %v, want [prod]", loaded.ProjectSecretsInFile)
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
	// Everything that is not a leak stays: removing it would delete a value
	// from a file a team may share, and drift never stored it for them.
	for _, wanted := range []string{`user = "deploy"`, `key_file = "~/.ssh/id_ed25519"`, "$STAGING_PW", `user = "webuser"`} {
		if !strings.Contains(written, wanted) {
			t.Fatalf("migration removed %q from the project config:\n%s", wanted, written)
		}
	}

	reloaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := reloaded.Hosts["prod"].Auth.Passphrase; got != "s3cret" {
		t.Fatalf("passphrase after migration = %q, want it from the store", got)
	}
	if len(reloaded.ProjectSecretsInFile) != 0 {
		t.Fatalf("ProjectSecretsInFile = %v, want empty after migration", reloaded.ProjectSecretsInFile)
	}
	if n, err := MigrateProjectSecrets(projectRoot); err != nil || n != 0 {
		t.Fatalf("second migration = (%d, %v), want (0, nil)", n, err)
	}
}

func TestMigrateKeepsAccessThatIsAlreadyStored(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()

	// The user configured this host through drift, so its access is stored.
	cfg := &MergedConfig{ProjectRoot: projectRoot}
	if err := SaveProjectHost(cfg, Host{
		Name: "prod", Hostname: "example.com",
		User: "webuser", Auth: Auth{Type: "keyfile", KeyFile: "~/.ssh/id_ed25519"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	// Then someone pasted a passphrase into the project config by hand.
	writeProjectConfig(t, projectRoot, `[[hosts]]
name = "prod"
hostname = "example.com"

  [hosts.auth]
  passphrase = "s3cret"
`)

	if _, err := MigrateProjectSecrets(projectRoot); err != nil {
		t.Fatalf("MigrateProjectSecrets returned error: %v", err)
	}

	store, err := loadAccess()
	if err != nil {
		t.Fatalf("loadAccess returned error: %v", err)
	}
	if len(store.Access) != 1 {
		t.Fatalf("store holds %d entries, want 1: %+v", len(store.Access), store.Access)
	}
	got := store.Access[0]
	if got.Auth.Passphrase != "s3cret" {
		t.Fatalf("the migrated passphrase is missing: %+v", got)
	}
	if got.User != "webuser" || got.Auth.KeyFile != "~/.ssh/id_ed25519" || got.Auth.Type != "keyfile" {
		t.Fatalf("migration overwrote the stored access: %+v", got)
	}
}

func TestMigrateFoldsInA016SecretsFile(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	writeProjectConfig(t, projectRoot, `[[hosts]]
name = "prod"
hostname = "example.com"
`)
	if err := os.MkdirAll(Dir(), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	legacy := "[[secrets]]\n  project = \"" + projectRoot + "\"\n  host = \"prod\"\n  password = \"hunter2\"\n"
	if err := os.WriteFile(legacySecretsPath(), []byte(legacy), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}

	if _, err := MigrateProjectSecrets(projectRoot); err != nil {
		t.Fatalf("MigrateProjectSecrets returned error: %v", err)
	}

	if _, err := os.Stat(legacySecretsPath()); !os.IsNotExist(err) {
		t.Fatal("secrets.toml was not removed after being folded in")
	}
	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := loaded.Hosts["prod"].Auth.Password; got != "hunter2" {
		t.Fatalf("password after folding secrets.toml = %q, want hunter2", got)
	}
	if n, err := MigrateProjectSecrets(projectRoot); err != nil || n != 0 {
		t.Fatalf("second migration = (%d, %v), want (0, nil)", n, err)
	}
}

func TestMigrateWithoutAConfigDoesNothing(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	if n, err := MigrateProjectSecrets(t.TempDir()); err != nil || n != 0 {
		t.Fatalf("migration without a config = (%d, %v), want (0, nil)", n, err)
	}
	if n, err := MigrateProjectSecrets(""); err != nil || n != 0 {
		t.Fatalf("migration without a root = (%d, %v), want (0, nil)", n, err)
	}
	if _, err := os.Stat(accessPath()); err == nil {
		t.Fatal("a no-op migration created an access store")
	}
}

func TestAccessStoreFilePermissions(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Auth: Auth{Type: "password", Password: "hunter2"}}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	info, err := os.Stat(accessPath())
	if err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
	if perm := info.Mode().Perm(); perm != 0o600 {
		t.Fatalf("access.toml permissions = %o, want 600", perm)
	}
	dir, err := os.Stat(filepath.Dir(accessPath()))
	if err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
	if perm := dir.Mode().Perm(); perm != 0o700 {
		t.Fatalf("config dir permissions = %o, want 700", perm)
	}

	// The atomic write must not leave its temporary file behind.
	entries, err := os.ReadDir(Dir())
	if err != nil {
		t.Fatalf("ReadDir returned error: %v", err)
	}
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), ".access.toml") {
			t.Fatalf("atomic write left %q behind", e.Name())
		}
	}
}

func TestEnvReferenceTypedIntoTheFormGoesToTheStore(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	projectRoot := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: projectRoot}

	if err := SaveProjectHost(cfg, Host{
		Name: "prod",
		Auth: Auth{Type: "password", Password: "$DEPLOY_PW"},
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	if written := readProjectConfig(t, projectRoot); strings.Contains(written, "$DEPLOY_PW") {
		t.Fatalf("an $ENV reference from the host form was written into the project:\n%s", written)
	}
	loaded, err := Load(projectRoot)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := loaded.Hosts["prod"].Auth.Password; got != "$DEPLOY_PW" {
		t.Fatalf("password = %q, want the $ENV reference from the store", got)
	}
}
