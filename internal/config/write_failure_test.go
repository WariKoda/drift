package config

import (
	"os"
	"path/filepath"
	"testing"
)

// blockConfigDir points $XDG_CONFIG_HOME at a path whose parent is a regular
// file, so every MkdirAll below it fails. Unlike a permission bit this also
// holds when the tests run as root.
func blockConfigDir(t *testing.T) {
	t.Helper()
	blocker := filepath.Join(t.TempDir(), "not-a-directory")
	if err := os.WriteFile(blocker, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(blocker, "config"))
}

func TestFailedGlobalSaveLeavesConfigUnchanged(t *testing.T) {
	blockConfigDir(t)
	cfg := &MergedConfig{GlobalHosts: []Host{{Name: "prod", Hostname: "example.com"}}}
	rebuildMerged(cfg)

	if err := SaveGlobalHost(cfg, Host{Name: "staging", Hostname: "staging.example.com"}, ""); err == nil {
		t.Fatal("SaveGlobalHost returned no error although the file could not be written")
	}

	if len(cfg.GlobalHosts) != 1 {
		t.Fatalf("GlobalHosts = %+v, want the single host it started with", cfg.GlobalHosts)
	}
	if _, ok := cfg.Hosts["staging"]; ok {
		t.Fatal("the failed host is visible in the merged view")
	}
}

func TestFailedGlobalDeleteLeavesConfigUnchanged(t *testing.T) {
	blockConfigDir(t)
	cfg := &MergedConfig{GlobalHosts: []Host{{Name: "prod", Hostname: "example.com"}}}
	rebuildMerged(cfg)

	if err := DeleteGlobalHost(cfg, "prod"); err == nil {
		t.Fatal("DeleteGlobalHost returned no error although the file could not be written")
	}

	if _, ok := cfg.Hosts["prod"]; !ok {
		t.Fatal("the host disappeared from the merged view although the delete failed")
	}
}

func TestFailedProjectSaveLeavesConfigUnchanged(t *testing.T) {
	blockConfigDir(t)
	cfg := &MergedConfig{ProjectSlug: "shop", ProjectHosts: []Host{{Name: "prod", Hostname: "example.com"}}}
	rebuildMerged(cfg)

	if err := SaveProjectHost(cfg, Host{Name: "staging", Hostname: "staging.example.com"}, ""); err == nil {
		t.Fatal("SaveProjectHost returned no error although the store could not be written")
	}

	if len(cfg.ProjectHosts) != 1 {
		t.Fatalf("ProjectHosts = %+v, want the single host it started with", cfg.ProjectHosts)
	}
	if _, ok := cfg.Hosts["staging"]; ok {
		t.Fatal("the failed host is visible in the merged view")
	}
}
