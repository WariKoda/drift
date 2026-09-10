package config

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestHostNameCollisionDoesNotMutate(t *testing.T) {
	for _, scope := range []string{"global", "project", "project-disk", "project-memory"} {
		for _, oldName := range []string{"", "original"} {
			t.Run(scope+"/"+oldName, func(t *testing.T) {
				isolate(t)
				cfg := &MergedConfig{ProjectSlug: "shop"}
				save := SaveGlobalHost
				path := filepath.Join(Dir(), "config.toml")
				if scope != "global" {
					save = SaveProjectHost
					path = filepath.Join(Dir(), "projects", "shop.toml")
				}
				for _, name := range []string{"original", "taken"} {
					if err := save(cfg, Host{Name: name, Hostname: name + ".example.com"}, ""); err != nil {
						t.Fatal(err)
					}
				}
				if scope == "project-disk" {
					cfg.ProjectHosts = cfg.ProjectHosts[:1]
					rebuildMerged(cfg)
				}
				if scope == "project-memory" {
					if err := writeProjectStore("shop", ProjectConfig{Hosts: cfg.ProjectHosts[:1]}); err != nil {
						t.Fatal(err)
					}
				}
				before, err := json.Marshal(cfg)
				if err != nil {
					t.Fatal(err)
				}
				diskBefore, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				err = save(cfg, Host{Name: "taken", Hostname: "replacement.example.com"}, oldName)
				if err == nil || !strings.Contains(err.Error(), "already exists") {
					t.Fatalf("expected collision, got %v", err)
				}
				after, err := json.Marshal(cfg)
				if err != nil {
					t.Fatal(err)
				}
				diskAfter, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				if !bytes.Equal(before, after) || !bytes.Equal(diskBefore, diskAfter) {
					t.Fatal("collision mutated config or disk")
				}
			})
		}
	}
}

func TestHostNamesAreScopedAndEditsExcludeOriginal(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{ProjectSlug: "shop"}
	for _, save := range []func(*MergedConfig, Host, string) error{SaveGlobalHost, SaveProjectHost} {
		if err := save(cfg, Host{Name: "shared", Hostname: "first.example.com"}, ""); err != nil {
			t.Fatal(err)
		}
		if err := save(cfg, Host{Name: "shared", Hostname: "edited.example.com"}, "shared"); err != nil {
			t.Fatal(err)
		}
		if err := save(cfg, Host{Name: "renamed", Hostname: "edited.example.com"}, "shared"); err != nil {
			t.Fatal(err)
		}
	}
	if len(cfg.GlobalHosts) != 1 || len(cfg.ProjectHosts) != 1 || cfg.GlobalHosts[0].Name != "renamed" || cfg.ProjectHosts[0].Name != "renamed" {
		t.Fatal("cross-scope rename did not preserve both hosts")
	}
	if err := SaveProjectHost(cfg, Host{Name: "other"}, ""); err != nil {
		t.Fatal(err)
	}
	projectPath := filepath.Join(Dir(), "projects", "shop.toml")
	projectBefore, err := os.ReadFile(projectPath)
	if err != nil {
		t.Fatal(err)
	}
	if err := SaveGlobalHost(cfg, Host{Name: "other"}, ""); err != nil {
		t.Fatal(err)
	}
	projectAfter, err := os.ReadFile(projectPath)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(projectBefore, projectAfter) {
		t.Fatal("global create changed project file")
	}
	globalBefore, err := os.ReadFile(filepath.Join(Dir(), "config.toml"))
	if err != nil {
		t.Fatal(err)
	}
	if err := SaveProjectHost(cfg, Host{Name: "renamed", Hostname: "project.example.com"}, "renamed"); err != nil {
		t.Fatal(err)
	}
	if cfg.GlobalHosts[0].Hostname != "edited.example.com" {
		t.Fatal("project edit mutated global host")
	}
	globalAfter, err := os.ReadFile(filepath.Join(Dir(), "config.toml"))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(globalBefore, globalAfter) {
		t.Fatal("project edit changed global file")
	}
	loaded, err := Load(t.TempDir(), "shop")
	if err != nil {
		t.Fatal(err)
	}
	if len(loaded.GlobalHosts) != 2 || len(loaded.ProjectHosts) != 2 || loaded.Hosts["renamed"].Hostname != "project.example.com" {
		t.Fatal("scoped hosts did not survive reload")
	}
}
