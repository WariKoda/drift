package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSaveGlobalHostPreservesDefaults(t *testing.T) {
	isolate(t)

	cfg := &MergedConfig{
		GlobalDefaults: Defaults{Port: 22, User: "deploy"},
		GlobalHosts: []Host{{
			Name:     "prod",
			Hostname: "example.com",
			Port:     22,
			User:     "deploy",
			RootPath: "/var/www",
		}},
	}

	if err := SaveGlobalHost(cfg, Host{
		Name:     "staging",
		Hostname: "staging.example.com",
		Port:     22,
		User:     "deploy",
		RootPath: "/srv/app",
	}, ""); err != nil {
		t.Fatalf("SaveGlobalHost returned error: %v", err)
	}

	data, err := os.ReadFile(filepath.Join(os.Getenv("XDG_CONFIG_HOME"), "drift", "config.toml"))
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}
	text := string(data)
	if !strings.Contains(text, "[defaults]") {
		t.Fatalf("written global config is missing [defaults]:\n%s", text)
	}
	if !strings.Contains(text, "port = 22") || !strings.Contains(text, "user = \"deploy\"") {
		t.Fatalf("written global config lost defaults:\n%s", text)
	}
}

func TestSaveProjectHostWritesTheStore(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{ProjectRoot: t.TempDir(), ProjectSlug: "shop"}

	if err := SaveProjectHost(cfg, Host{
		Name:        "staging",
		Hostname:    "shop.example.com",
		Port:        21,
		User:        "webuser",
		Protocol:    "ftps",
		InsecureTLS: true,
		Auth:        Auth{Type: "password", Password: "hunter2"},
		RootPath:    "/var/www",
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	// Nothing is written into the project.
	if _, err := os.Stat(filepath.Join(cfg.ProjectRoot, ".drift")); !os.IsNotExist(err) {
		t.Fatalf(".drift was created in the project, Stat error = %v", err)
	}

	path, err := projectStorePath("shop")
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}
	for _, want := range []string{`hostname = "shop.example.com"`, `user = "webuser"`, "insecure_tls = true", `password = "hunter2"`} {
		if !strings.Contains(string(data), want) {
			t.Fatalf("the store is missing %q:\n%s", want, data)
		}
	}

	loaded, err := Load(cfg.ProjectRoot, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	h := loaded.Hosts["staging"]
	if h.User != "webuser" || !h.InsecureTLS || h.Auth.Password != "hunter2" || h.Protocol != "ftps" {
		t.Fatalf("host did not round-trip through the store: %+v", h)
	}
}

func TestStoreFilePermissions(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{ProjectRoot: t.TempDir(), ProjectSlug: "shop"}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Auth: Auth{Type: "password", Password: "hunter2"}}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	path, err := projectStorePath("shop")
	if err != nil {
		t.Fatal(err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
	if perm := info.Mode().Perm(); perm != 0o600 {
		t.Fatalf("store permissions = %o, want 600", perm)
	}
	dir, err := os.Stat(filepath.Dir(path))
	if err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
	if perm := dir.Mode().Perm(); perm != 0o700 {
		t.Fatalf("projects dir permissions = %o, want 700", perm)
	}
}

func TestSaveProjectHostWithoutASlugFails(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{ProjectRoot: t.TempDir()}
	if err := SaveProjectHost(cfg, Host{Name: "prod"}, ""); err == nil {
		t.Fatal("SaveProjectHost returned nil without a project slug")
	}
	if len(cfg.ProjectHosts) != 0 {
		t.Fatalf("SaveProjectHost mutated the config without a slug: %#v", cfg.ProjectHosts)
	}
}

func TestSaveProjectHostPreservesDefaultsAndMappings(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{
		ProjectRoot:     t.TempDir(),
		ProjectSlug:     "shop",
		ProjectDefaults: Defaults{Port: 21, User: "webuser"},
		ProjectHosts: []Host{{
			Name:     "staging",
			Hostname: "staging.example.com",
			Port:     21,
			User:     "webuser",
			RootPath: "/var/www",
		}},
		Mappings: []Mapping{{Local: "plugins/plugin1", Remote: "html/custom/plugins/plugin1"}},
	}

	if err := SaveProjectHost(cfg, Host{
		Name:     "prod",
		Hostname: "prod.example.com",
		Port:     21,
		User:     "webuser",
		RootPath: "/srv/app",
	}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	path, err := projectStorePath("shop")
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}
	text := string(data)
	if !strings.Contains(text, "[defaults]") || !strings.Contains(text, `user = "webuser"`) {
		t.Fatalf("written store lost defaults:\n%s", text)
	}
	if !strings.Contains(text, `local = "plugins/plugin1"`) {
		t.Fatalf("written store lost mappings:\n%s", text)
	}
	if got := strings.Count(text, "[[hosts]]"); got != 2 {
		t.Fatalf("store holds %d hosts, want 2:\n%s", got, text)
	}
}

func TestSavingOneHostLeavesTheOthersUntouched(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	if err := writeProjectStore("shop", ProjectConfig{
		Defaults: Defaults{Port: 2222, User: "deploy"},
		Hosts:    []Host{{Name: "staging", Hostname: "staging.example.com", Auth: Auth{Type: "agent"}}},
	}); err != nil {
		t.Fatalf("writeProjectStore returned error: %v", err)
	}

	cfg, err := Load(root, "shop")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if got := cfg.Hosts["staging"]; got.Port != 2222 || got.User != "deploy" {
		t.Fatalf("defaults were not applied in memory: %+v", got)
	}

	if err := SaveProjectHost(cfg, Host{Name: "prod", Hostname: "example.com", Port: 22}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	stored, err := loadProjectStore("shop")
	if err != nil || stored == nil {
		t.Fatalf("loadProjectStore returned (%v, %v)", stored, err)
	}
	i := hostIndex(stored.Hosts, "staging")
	if i < 0 {
		t.Fatal("staging vanished from the store")
	}
	// [defaults] must keep doing the work; a save may not bake it into records.
	if stored.Hosts[i].Port != 0 || stored.Hosts[i].User != "" {
		t.Fatalf("defaults were baked into the untouched host: %+v", stored.Hosts[i])
	}
}

func TestDeleteProjectHostRewritesTheStore(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: root, ProjectSlug: "shop"}
	for _, h := range []Host{{Name: "prod", Hostname: "a.example.com"}, {Name: "staging", Hostname: "b.example.com"}} {
		if err := SaveProjectHost(cfg, h, ""); err != nil {
			t.Fatalf("SaveProjectHost returned error: %v", err)
		}
	}
	if err := DeleteProjectHost(cfg, "prod"); err != nil {
		t.Fatalf("DeleteProjectHost returned error: %v", err)
	}

	stored, err := loadProjectStore("shop")
	if err != nil {
		t.Fatalf("loadProjectStore returned error: %v", err)
	}
	if len(stored.Hosts) != 1 || stored.Hosts[0].Name != "staging" {
		t.Fatalf("store after delete = %+v, want only staging", stored.Hosts)
	}
}

func TestLoadWithoutASlugSeesOnlyGlobalHosts(t *testing.T) {
	isolate(t)
	if err := writeProjectStore("shop", ProjectConfig{Hosts: []Host{{Name: "prod", Hostname: "example.com"}}}); err != nil {
		t.Fatalf("writeProjectStore returned error: %v", err)
	}
	cfg := &MergedConfig{}
	if err := SaveGlobalHost(cfg, Host{Name: "global", Hostname: "global.example.com"}, ""); err != nil {
		t.Fatalf("SaveGlobalHost returned error: %v", err)
	}

	loaded, err := Load(t.TempDir(), "")
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if _, ok := loaded.Hosts["prod"]; ok {
		t.Fatal("a project store was loaded without a slug")
	}
	if _, ok := loaded.Hosts["global"]; !ok {
		t.Fatal("global hosts are missing")
	}
}

func TestSaveGlobalHostRejectsInvalidMappingWithoutMutation(t *testing.T) {
	configHome := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)
	cfg := &MergedConfig{}

	err := SaveGlobalHost(cfg, Host{
		Name:     "prod",
		Hostname: "example.com",
		RootPath: "/var/www",
		Mappings: []Mapping{{Local: "../outside", Remote: "app"}},
	}, "")
	if err == nil {
		t.Fatal("SaveGlobalHost returned nil for invalid mapping")
	}
	if len(cfg.GlobalHosts) != 0 || len(cfg.Hosts) != 0 {
		t.Fatalf("SaveGlobalHost mutated config after validation failure: %#v", cfg)
	}
	if _, statErr := os.Stat(filepath.Join(configHome, "drift", "config.toml")); !os.IsNotExist(statErr) {
		t.Fatalf("invalid host was written to disk, Stat error = %v", statErr)
	}
}

func TestSaveProjectHostRejectsInvalidProjectMappingsWithoutMutation(t *testing.T) {
	isolate(t)
	projectRoot := t.TempDir()
	cfg := &MergedConfig{
		Mappings:    []Mapping{{Local: "app", Remote: "/absolute"}},
		ProjectRoot: projectRoot,
		ProjectSlug: "shop",
	}

	err := SaveProjectHost(cfg, Host{
		Name:     "prod",
		Hostname: "example.com",
		RootPath: "/var/www",
	}, "")
	if err == nil {
		t.Fatal("SaveProjectHost returned nil for invalid project mapping")
	}
	if len(cfg.ProjectHosts) != 0 || len(cfg.Hosts) != 0 {
		t.Fatalf("SaveProjectHost mutated config after validation failure: %#v", cfg)
	}
	path, err := projectStorePath("shop")
	if err != nil {
		t.Fatal(err)
	}
	if _, statErr := os.Stat(path); !os.IsNotExist(statErr) {
		t.Fatalf("invalid project config was written to disk, Stat error = %v", statErr)
	}
}
