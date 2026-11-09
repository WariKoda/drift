package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDiffPreferencesRoundTripAndGlobalScope(t *testing.T) {
	isolate(t)
	root := t.TempDir()
	cfg, err := Load(root, "project")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.UI.Diff != nil {
		t.Fatal("missing settings must retain default layout")
	}
	cfg.UI.Diff = &DiffPreferences{Layout: "auto", Syntax: true, Continuous: true}
	if err := SaveGlobalHost(cfg, Host{Name: "test", Hostname: "example.test"}, ""); err != nil {
		t.Fatal(err)
	}
	loaded, err := Load(root, "project")
	if err != nil {
		t.Fatal(err)
	}
	if loaded.UI.Diff == nil || loaded.UI.Diff.Layout != "auto" || !loaded.UI.Diff.Syntax || !loaded.UI.Diff.Continuous {
		t.Fatal("global write lost diff preferences")
	}
	if err := os.MkdirAll(filepath.Join(Dir(), "projects"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(Dir(), "projects", "project.toml"), []byte("[ui.diff]\nlayout='split'\ncontinuous=false\n"), 0600); err != nil {
		t.Fatal(err)
	}
	loaded, err = Load(root, "project")
	if err != nil {
		t.Fatal(err)
	}
	if loaded.UI.Diff.Layout != "auto" || !loaded.UI.Diff.Syntax || !loaded.UI.Diff.Continuous {
		t.Fatal("project settings overrode global diff preferences")
	}
}

func TestDiffPreferencesValidation(t *testing.T) {
	for _, layout := range []string{"", "unified", "split", "auto"} {
		if err := (DiffPreferences{Layout: layout}).Validate(); err != nil {
			t.Fatal(err)
		}
	}
	if err := (DiffPreferences{Layout: "unexpected"}).Validate(); err == nil {
		t.Fatal("invalid layout accepted")
	}
	isolate(t)
	if err := writeGlobal(GlobalConfig{UI: UI{Diff: &DiffPreferences{Layout: "unexpected"}}}); err == nil {
		t.Fatal("writer accepted invalid layout")
	}
	if err := os.MkdirAll(Dir(), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(globalConfigPath(), []byte("[ui.diff]\nlayout='unexpected'\n"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := Load(t.TempDir(), ""); err == nil {
		t.Fatal("loader accepted invalid layout")
	}
}
