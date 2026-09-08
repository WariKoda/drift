package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestKeepAliveDuration(t *testing.T) {
	for _, seconds := range []int{-1, 0, 1, 60, 86400} {
		t.Run(fmt.Sprint(seconds), func(t *testing.T) {
			h := Host{}
			want := 60 * time.Second
			if seconds >= 0 {
				h.KeepAliveInterval = &seconds
				want = time.Duration(seconds) * time.Second
			}
			if got := h.KeepAliveDuration(); got != want {
				t.Fatalf("KeepAliveDuration() = %v, want %v", got, want)
			}
			if seconds < 0 && h.KeepAliveInterval != nil {
				t.Fatal("resolving default populated the stored interval")
			}
		})
	}
	if KeepAliveTimeout != 15*time.Second {
		t.Fatalf("KeepAliveTimeout = %v, want 15s", KeepAliveTimeout)
	}
}

func TestKeepAliveLoadValidation(t *testing.T) {
	for _, scope := range []string{"global", "project"} {
		for _, value := range []string{"", "0", "1", "86400", "-1", "86401", `"60"`, "true", "1.5", "[]", "9223372036854775808"} {
			t.Run(scope+"/"+value, func(t *testing.T) {
				isolate(t)
				path := globalConfigPath()
				if scope == "project" {
					var err error
					path, err = projectStorePath("shop")
					if err != nil {
						t.Fatal(err)
					}
				}
				if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
					t.Fatal(err)
				}
				text := "[[hosts]]\nname = \"prod\"\n"
				if value != "" {
					text += "keep_alive_interval = " + value + "\n"
				}
				if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
					t.Fatal(err)
				}
				cfg, err := Load(t.TempDir(), "shop")
				valid := value == "" || value == "0" || value == "1" || value == "86400"
				if !valid {
					if err == nil || !strings.Contains(err.Error(), "keep_alive_interval") {
						t.Fatalf("Load error = %v, want interval error", err)
					}
					return
				}
				if err != nil {
					t.Fatal(err)
				}
				h := cfg.Hosts["prod"]
				if value == "" {
					if h.KeepAliveInterval != nil || h.KeepAliveDuration() != 60*time.Second {
						t.Fatalf("missing interval did not use default: %+v", h)
					}
				} else if h.KeepAliveInterval == nil || fmt.Sprint(*h.KeepAliveInterval) != value {
					t.Fatalf("loaded interval does not match %s: %+v", value, h)
				}
			})
		}
	}
}

func TestKeepAliveRoundTrip(t *testing.T) {
	for _, scope := range []HostScope{ScopeGlobal, ScopeProject} {
		t.Run(fmt.Sprint(scope), func(t *testing.T) {
			isolate(t)
			root := t.TempDir()
			cfg := &MergedConfig{ProjectRoot: root, ProjectSlug: "shop"}
			save, remove := SaveGlobalHost, DeleteGlobalHost
			path := globalConfigPath()
			if scope == ScopeProject {
				save, remove = SaveProjectHost, DeleteProjectHost
				var err error
				path, err = projectStorePath("shop")
				if err != nil {
					t.Fatal(err)
				}
			}
			zero, positive, maximum := 0, 17, 86400
			for _, h := range []Host{
				{Name: "default"},
				{Name: "off", KeepAliveInterval: &zero},
				{Name: "custom", KeepAliveInterval: &positive},
				{Name: "maximum", KeepAliveInterval: &maximum},
			} {
				if err := save(cfg, h, ""); err != nil {
					t.Fatal(err)
				}
			}
			cfg, err := Load(root, "shop")
			if err != nil {
				t.Fatal(err)
			}
			// Saving and deleting an unrelated host must not materialize defaults.
			if err := save(cfg, Host{Name: "unrelated"}, ""); err != nil {
				t.Fatal(err)
			}
			if err := remove(cfg, "unrelated"); err != nil {
				t.Fatal(err)
			}
			for _, name := range []string{"default", "off", "custom", "maximum"} {
				if err := save(cfg, cfg.Hosts[name], name); err != nil {
					t.Fatal(err)
				}
			}
			loaded, err := Load(root, "shop")
			if err != nil {
				t.Fatal(err)
			}
			for name, want := range map[string]*int{"default": nil, "off": &zero, "custom": &positive, "maximum": &maximum} {
				got := loaded.Hosts[name].KeepAliveInterval
				if (got == nil) != (want == nil) || (got != nil && *got != *want) {
					t.Fatalf("host %s interval = %v, want %v", name, got, want)
				}
			}
			data, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			if strings.Count(string(data), "keep_alive_interval") != 3 || !strings.Contains(string(data), "keep_alive_interval = 0") {
				t.Fatalf("written intervals lost zero or populated absent values:\n%s", data)
			}
			entries, err := os.ReadDir(root)
			if err != nil || len(entries) != 0 {
				t.Fatalf("project directory changed: %v, %v", entries, err)
			}
		})
	}
}

func TestKeepAliveWriteValidation(t *testing.T) {
	for _, seconds := range []int{-1, 86401} {
		bad := Host{Name: "invalid", KeepAliveInterval: &seconds}
		for name, write := range map[string]func(*MergedConfig) error{
			"save global":  func(c *MergedConfig) error { return SaveGlobalHost(c, bad, "") },
			"save project": func(c *MergedConfig) error { return SaveProjectHost(c, bad, "") },
			"write global": func(c *MergedConfig) error { return writeGlobal(GlobalConfig{Hosts: []Host{bad}}) },
			"write project": func(c *MergedConfig) error {
				return writeProjectStore(c.ProjectSlug, ProjectConfig{Hosts: []Host{bad}})
			},
			"save global with invalid sibling": func(c *MergedConfig) error {
				c.GlobalHosts = []Host{bad}
				return SaveGlobalHost(c, Host{Name: "valid"}, "")
			},
			"save project with invalid sibling": func(c *MergedConfig) error {
				c.ProjectHosts = []Host{bad}
				return SaveProjectHost(c, Host{Name: "valid"}, "")
			},
			"delete global with invalid sibling": func(c *MergedConfig) error {
				c.GlobalHosts = []Host{bad}
				return DeleteGlobalHost(c, "other")
			},
			"delete project with invalid sibling": func(c *MergedConfig) error {
				c.ProjectHosts = []Host{bad}
				return DeleteProjectHost(c, "other")
			},
		} {
			t.Run(fmt.Sprint(seconds)+"/"+name, func(t *testing.T) {
				isolate(t)
				cfg := &MergedConfig{ProjectRoot: t.TempDir(), ProjectSlug: "shop"}
				if err := write(cfg); err == nil || !strings.Contains(err.Error(), "keep_alive_interval") {
					t.Fatalf("write error = %v, want interval error", err)
				}
				if _, err := os.Stat(Dir()); !os.IsNotExist(err) {
					t.Fatalf("invalid write touched config directory: %v", err)
				}
			})
		}
	}
}

func TestKeepAliveProjectOverrideDoesNotInheritGlobalInterval(t *testing.T) {
	isolate(t)
	zero := 0
	cfg := &MergedConfig{ProjectRoot: t.TempDir(), ProjectSlug: "shop"}
	if err := SaveGlobalHost(cfg, Host{Name: "prod", KeepAliveInterval: &zero}, ""); err != nil {
		t.Fatal(err)
	}
	if err := SaveProjectHost(cfg, Host{Name: "prod"}, ""); err != nil {
		t.Fatal(err)
	}
	loaded, err := Load(cfg.ProjectRoot, cfg.ProjectSlug)
	if err != nil {
		t.Fatal(err)
	}
	if h := loaded.Hosts["prod"]; h.KeepAliveInterval != nil || h.KeepAliveDuration() != 60*time.Second {
		t.Fatalf("project host inherited global interval: %+v", h)
	}
}
