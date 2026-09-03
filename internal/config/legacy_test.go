package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// legacyConfig is a project config as drift wrote it before 0.1.6-alpha:
// hosts, mappings and credentials all in the project.
const legacyConfig = `[defaults]
user = "deploy"

[[hosts]]
name = "prod"
hostname = "example.com"
root_path = "/var/www"

  [hosts.auth]
  type = "keyfile"
  key_file = "~/.ssh/id_ed25519"
  passphrase = "s3cret"

  [[hosts.mappings]]
  local = "src"
  remote = "html"

[[mappings]]
local = "plugins"
remote = "custom/plugins"
`

func putLegacyConfig(t *testing.T, root, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(root, ".drift"), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	if err := os.WriteFile(legacyProjectConfig(root), []byte(body), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
}

func putLegacyStore(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(Dir(), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
}

// TestMigrateFromProjectConfig covers state (a): everything in the project.
func TestMigrateFromProjectConfig(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, legacyConfig)

	before, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if !before.LegacyFiles {
		t.Fatal("Load did not report the leftover project config")
	}
	if len(before.ProjectHosts) != 0 {
		t.Fatal("the project config was read as if it were the store")
	}

	res, err := MigrateProjectToStore(root, "shop")
	if err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}
	if res.Hosts != 1 || !res.ProjectFile || !res.Moved() {
		t.Fatalf("result = %+v, want one host from a project file", res)
	}

	after, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	h := after.Hosts["prod"]
	if h.Auth.Passphrase != "s3cret" || h.Auth.KeyFile != "~/.ssh/id_ed25519" || h.Auth.Type != "keyfile" {
		t.Fatalf("auth did not survive the migration: %+v", h.Auth)
	}
	if h.User != "deploy" {
		t.Fatalf("user = %q, want it from [defaults]", h.User)
	}
	if len(h.Mappings) != 1 || h.Mappings[0].Local != "src" {
		t.Fatalf("host mappings did not survive: %+v", h.Mappings)
	}
	if len(after.Mappings) != 1 || after.Mappings[0].Local != "plugins" {
		t.Fatalf("project mappings did not survive: %+v", after.Mappings)
	}
	if after.LegacyFiles {
		t.Fatal("Load still reports leftovers after the migration")
	}

	// Nothing of drift's is left in the project.
	if _, err := os.Stat(filepath.Join(root, ".drift")); !os.IsNotExist(err) {
		t.Fatalf(".drift survived the migration, Stat error = %v", err)
	}

	// State (d): a second run is a no-op.
	res, err = MigrateProjectToStore(root, "shop")
	if err != nil || res.Moved() {
		t.Fatalf("second migration = (%+v, %v), want nothing moved", res, err)
	}
}

// TestMigrateFromSecretsStore covers state (b): 0.1.6-alpha.
func TestMigrateFromSecretsStore(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, `[[hosts]]
name = "prod"
hostname = "example.com"
user = "deploy"

  [hosts.auth]
  type = "password"
`)
	putLegacyStore(t, secretsPath(), "[[secrets]]\n  project = \""+root+"\"\n  host = \"prod\"\n  password = \"hunter2\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}

	after, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := after.Hosts["prod"].Auth.Password; got != "hunter2" {
		t.Fatalf("password = %q, want it from secrets.toml", got)
	}
	if _, err := os.Stat(secretsPath()); !os.IsNotExist(err) {
		t.Fatal("secrets.toml was not removed once it held nothing")
	}
}

// TestMigrateFromAccessStore covers state (c): 0.1.7-alpha.
func TestMigrateFromAccessStore(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, `[[hosts]]
name = "prod"
hostname = "example.com"
port = 21
protocol = "ftp"
`)
	putLegacyStore(t, accessPath(), "[[access]]\n  project = \""+root+"\"\n  host = \"prod\"\n  user = \"webuser\"\n  insecure_tls = true\n  [access.auth]\n    type = \"password\"\n    password = \"hunter2\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}

	after, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	h := after.Hosts["prod"]
	if h.User != "webuser" || !h.InsecureTLS || h.Auth.Password != "hunter2" {
		t.Fatalf("access did not come across: %+v", h)
	}
	if h.Protocol != "ftp" || h.Port != 21 {
		t.Fatalf("environment fields were lost: %+v", h)
	}
	if _, err := os.Stat(accessPath()); !os.IsNotExist(err) {
		t.Fatal("access.toml was not removed once it held nothing")
	}
}

func TestMigrateKeepsOtherProjectsInTheLegacyStores(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, "[[hosts]]\nname = \"prod\"\nhostname = \"example.com\"\n")
	putLegacyStore(t, accessPath(),
		"[[access]]\n  project = \""+root+"\"\n  host = \"prod\"\n  user = \"mine\"\n\n"+
			"[[access]]\n  project = \"/other/project\"\n  host = \"prod\"\n  user = \"theirs\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}

	data, err := os.ReadFile(accessPath())
	if err != nil {
		t.Fatalf("access.toml was removed although another project still used it: %v", err)
	}
	if strings.Contains(string(data), root) {
		t.Fatalf("this project's entry was left behind:\n%s", data)
	}
	if !strings.Contains(string(data), "theirs") {
		t.Fatalf("another project's entry was dropped:\n%s", data)
	}
}

func TestMigrateLeavesTheProjectConfigValuesInCharge(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, `[[hosts]]
name = "prod"
hostname = "example.com"
user = "from-project-config"
`)
	putLegacyStore(t, accessPath(), "[[access]]\n  project = \""+root+"\"\n  host = \"prod\"\n  user = \"from-access-store\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}

	after, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	// A person wrote the project config by hand; the store was drift's own.
	if got := after.Hosts["prod"].User; got != "from-project-config" {
		t.Fatalf("user = %q, want the project config's value", got)
	}
}

func TestMigrateRemovesDriftsOwnGitignoreButNotAForeignFile(t *testing.T) {
	isolate(t)

	own := t.TempDir()
	putLegacyConfig(t, own, "[[hosts]]\nname = \"prod\"\n")
	if err := os.WriteFile(filepath.Join(own, ".drift", ".gitignore"), []byte(projectGitignore), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
	if _, err := MigrateProjectToStore(own, "own"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}
	if _, err := os.Stat(filepath.Join(own, ".drift")); !os.IsNotExist(err) {
		t.Fatalf(".drift with only drift's own .gitignore survived, Stat error = %v", err)
	}

	foreign := t.TempDir()
	putLegacyConfig(t, foreign, "[[hosts]]\nname = \"prod\"\n")
	if err := os.WriteFile(filepath.Join(foreign, ".drift", "notes.md"), []byte("mine\n"), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
	if _, err := MigrateProjectToStore(foreign, "foreign"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}
	if _, err := os.Stat(filepath.Join(foreign, ".drift", "notes.md")); err != nil {
		t.Fatalf("a foreign file under .drift was removed: %v", err)
	}
	if _, err := os.Stat(legacyProjectConfig(foreign)); !os.IsNotExist(err) {
		t.Fatal("the project config itself was not removed")
	}
}

func TestMigrateWithoutASlugOrRootDoesNothing(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, "[[hosts]]\nname = \"prod\"\n")

	for _, tc := range []struct{ root, slug string }{{root, ""}, {"", "shop"}, {"", ""}} {
		res, err := MigrateProjectToStore(tc.root, tc.slug)
		if err != nil || res.Moved() {
			t.Fatalf("migration(%q, %q) = (%+v, %v), want nothing moved", tc.root, tc.slug, res, err)
		}
	}
	if _, err := os.Stat(legacyProjectConfig(root)); err != nil {
		t.Fatalf("the project config was touched without a slug: %v", err)
	}
}

func TestFindLegacyProjectRootWalksUp(t *testing.T) {
	root := t.TempDir()
	putLegacyConfig(t, root, "[[hosts]]\nname = \"prod\"\n")
	deep := filepath.Join(root, "a", "b")
	if err := os.MkdirAll(deep, 0o755); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}

	got, ok := FindLegacyProjectRoot(deep)
	if !ok || got != root {
		t.Fatalf("FindLegacyProjectRoot(%q) = (%q, %v), want (%q, true)", deep, got, ok, root)
	}
	if _, ok := FindLegacyProjectRoot(t.TempDir()); ok {
		t.Fatal("FindLegacyProjectRoot found a project where there is none")
	}
}

func TestMigrateTakesOverAccessWhenTheProjectConfigIsAlreadyGone(t *testing.T) {
	isolate(t)
	root := t.TempDir()

	// The store already knows the host; only its credential is still in the
	// old place. Deleting that entry unread would lose the password.
	if err := writeProjectStore("shop", ProjectConfig{
		Hosts: []Host{{Name: "prod", Hostname: "example.com", Auth: Auth{Type: "password"}}},
	}); err != nil {
		t.Fatalf("writeProjectStore returned error: %v", err)
	}
	putLegacyStore(t, secretsPath(), "[[secrets]]\n  project = \""+root+"\"\n  host = \"prod\"\n  password = \"hunter2\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}

	after, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := after.Hosts["prod"].Auth.Password; got != "hunter2" {
		t.Fatalf("password = %q, want it taken over from secrets.toml", got)
	}
	if _, err := os.Stat(secretsPath()); !os.IsNotExist(err) {
		t.Fatal("secrets.toml survived although its only entry was taken over")
	}
}

func TestMigrateKeepsAnEntryItCouldNotPlace(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, "[[hosts]]\nname = \"prod\"\nhostname = \"example.com\"\n")
	putLegacyStore(t, accessPath(),
		"[[access]]\n  project = \""+root+"\"\n  host = \"prod\"\n  user = \"mine\"\n\n"+
			"[[access]]\n  project = \""+root+"\"\n  host = \"a-host-nobody-knows\"\n  user = \"orphan\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}

	data, err := os.ReadFile(accessPath())
	if err != nil {
		t.Fatalf("access.toml was removed with an unplaced entry in it: %v", err)
	}
	if !strings.Contains(string(data), "orphan") {
		t.Fatalf("the unplaced entry was deleted unread:\n%s", data)
	}
	if strings.Contains(string(data), `user = "mine"`) {
		t.Fatalf("the entry that was taken over is still there:\n%s", data)
	}
}

func TestMigrateMatchesPathsLoosely(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	putLegacyConfig(t, root, "[[hosts]]\nname = \"prod\"\nhostname = \"example.com\"\n")
	// A trailing slash must not cost someone their credentials.
	putLegacyStore(t, secretsPath(), "[[secrets]]\n  project = \""+root+"/\"\n  host = \"prod\"\n  password = \"hunter2\"\n")

	if _, err := MigrateProjectToStore(root, "shop"); err != nil {
		t.Fatalf("MigrateProjectToStore returned error: %v", err)
	}
	after, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := after.Hosts["prod"].Auth.Password; got != "hunter2" {
		t.Fatalf("password = %q, want the entry with the trailing slash to match", got)
	}
}
