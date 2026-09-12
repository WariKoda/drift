package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestUIVisibilityDefaultsAndRoundTrip(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	cfg, err := Load(root, "")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.UI.ShowHidden || cfg.UI.ShowIgnored {
		t.Fatalf("visibility defaults = %+v, want both false", cfg.UI)
	}

	cfg.UI.ShowHidden = true
	cfg.UI.ShowIgnored = true
	if err := SaveGlobalHost(cfg, Host{Name: "test", Hostname: "example.test"}, ""); err != nil {
		t.Fatal(err)
	}
	reloaded, err := Load(root, "")
	if err != nil {
		t.Fatal(err)
	}
	if !reloaded.UI.ShowHidden || !reloaded.UI.ShowIgnored {
		t.Fatalf("visibility settings lost: %+v", reloaded.UI)
	}
	contents, err := os.ReadFile(filepath.Join(Dir(), "config.toml"))
	if err != nil {
		t.Fatal(err)
	}
	text := string(contents)
	if !strings.Contains(text, "show_hidden = true") || !strings.Contains(text, "show_ignored = true") {
		t.Fatalf("written config missing UI settings:\n%s", text)
	}
}
