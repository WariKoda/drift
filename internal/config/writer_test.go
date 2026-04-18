package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSaveGlobalHostPreservesDefaults(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

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

func TestSaveProjectHostPreservesDefaultsAndMappings(t *testing.T) {
	projectRoot := t.TempDir()
	cfg := &MergedConfig{
		ProjectDefaults: Defaults{Port: 21, User: "webuser"},
		ProjectHosts: []Host{{
			Name:     "staging",
			Hostname: "staging.example.com",
			Port:     21,
			User:     "webuser",
			RootPath: "/var/www",
		}},
		Mappings:    []Mapping{{Local: "plugins/plugin1", Remote: "html/custom/plugins/plugin1"}},
		ProjectRoot: projectRoot,
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

	data, err := os.ReadFile(filepath.Join(projectRoot, ".drift", "config.toml"))
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}
	text := string(data)
	if !strings.Contains(text, "[defaults]") {
		t.Fatalf("written project config is missing [defaults]:\n%s", text)
	}
	if !strings.Contains(text, "user = \"webuser\"") {
		t.Fatalf("written project config lost defaults:\n%s", text)
	}
	if !strings.Contains(text, "local = \"plugins/plugin1\"") || !strings.Contains(text, "remote = \"html/custom/plugins/plugin1\"") {
		t.Fatalf("written project config lost mappings:\n%s", text)
	}
}
