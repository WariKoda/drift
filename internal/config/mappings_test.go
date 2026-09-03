package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestValidateMappings(t *testing.T) {
	tests := []struct {
		name     string
		mappings []Mapping
		wantErr  string
	}{
		{
			name: "normal siblings",
			mappings: []Mapping{
				{Local: "plugins/one", Remote: "custom/plugins/one"},
				{Local: "themes/two", Remote: "custom/themes/two"},
			},
		},
		{
			name: "whole roots",
			mappings: []Mapping{
				{Local: ".", Remote: "."},
			},
		},
		{
			name: "mirrored nesting",
			mappings: []Mapping{
				{Local: "plugins", Remote: "custom/plugins"},
				{Local: "plugins/one", Remote: "custom/plugins/one"},
			},
		},
		{
			name: "clean relative paths",
			mappings: []Mapping{
				{Local: "plugins/./one", Remote: "custom/./plugins/one"},
			},
		},
		{
			name:     "empty local",
			mappings: []Mapping{{Local: " ", Remote: "custom/plugins"}},
			wantErr:  "must not be empty",
		},
		{
			name:     "empty remote",
			mappings: []Mapping{{Local: "plugins", Remote: ""}},
			wantErr:  "must not be empty",
		},
		{
			name:     "absolute local",
			mappings: []Mapping{{Local: "/tmp/plugins", Remote: "plugins"}},
			wantErr:  "relative to the project root",
		},
		{
			name:     "absolute remote",
			mappings: []Mapping{{Local: "plugins", Remote: "/srv/plugins"}},
			wantErr:  "relative to the host root",
		},
		{
			name:     "local traversal",
			mappings: []Mapping{{Local: "plugins/../../outside", Remote: "plugins"}},
			wantErr:  `must not contain ".." segments`,
		},
		{
			name:     "remote traversal",
			mappings: []Mapping{{Local: "plugins", Remote: "custom/../../outside"}},
			wantErr:  `must not contain ".." segments`,
		},
		{
			name: "duplicate normalized local",
			mappings: []Mapping{
				{Local: "plugins/./one", Remote: "remote/one"},
				{Local: "plugins/one", Remote: "remote/two"},
			},
			wantErr: "same local path",
		},
		{
			name: "duplicate normalized remote",
			mappings: []Mapping{
				{Local: "plugins/one", Remote: "remote/./one"},
				{Local: "plugins/two", Remote: "remote/one"},
			},
			wantErr: "same remote path",
		},
		{
			name: "local nesting maps to disjoint remote path",
			mappings: []Mapping{
				{Local: "plugins", Remote: "remote/plugins"},
				{Local: "plugins/one", Remote: "other/one"},
			},
			wantErr: "overlap differently",
		},
		{
			name: "remote nesting maps from disjoint local path",
			mappings: []Mapping{
				{Local: "plugins", Remote: "remote/plugins"},
				{Local: "themes", Remote: "remote/plugins/theme"},
			},
			wantErr: "overlap differently",
		},
		{
			name: "nesting direction is reversed",
			mappings: []Mapping{
				{Local: "plugins", Remote: "remote/plugins/one"},
				{Local: "plugins/one", Remote: "remote/plugins"},
			},
			wantErr: "overlap differently",
		},
		{
			name: "nested suffix differs",
			mappings: []Mapping{
				{Local: "plugins", Remote: "remote/plugins"},
				{Local: "plugins/one", Remote: "remote/plugins/two"},
			},
			wantErr: "overlap differently",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateMappings(tt.mappings)
			if tt.wantErr == "" {
				if err != nil {
					t.Fatalf("ValidateMappings returned error: %v", err)
				}
				return
			}
			if err == nil {
				t.Fatalf("ValidateMappings returned nil, want error containing %q", tt.wantErr)
			}
			if !strings.Contains(err.Error(), tt.wantErr) {
				t.Fatalf("ValidateMappings error = %q, want substring %q", err, tt.wantErr)
			}
		})
	}
}

func TestLoadRejectsInvalidMappings(t *testing.T) {
	tests := []struct {
		name       string
		configPath func(configHome, projectRoot string) string
		content    string
		wantErr    string
	}{
		{
			name: "global host mapping",
			configPath: func(configHome, _ string) string {
				return filepath.Join(configHome, "drift", "config.toml")
			},
			content: `
[[hosts]]
name = "prod"
hostname = "example.com"
root_path = "/var/www"

[[hosts.mappings]]
local = "../outside"
remote = "app"
`,
			wantErr: `global host "prod" mappings`,
		},
		{
			name: "project mapping",
			configPath: func(configHome, _ string) string {
				return filepath.Join(configHome, "drift", "projects", "shop.toml")
			},
			content: `
[[mappings]]
local = "app"
remote = "../outside"
`,
			wantErr: "project mappings",
		},
		{
			name: "project host mapping",
			configPath: func(configHome, _ string) string {
				return filepath.Join(configHome, "drift", "projects", "shop.toml")
			},
			content: `
[[hosts]]
name = "prod"
hostname = "example.com"
root_path = "/var/www"

[[hosts.mappings]]
local = "/outside"
remote = "app"
`,
			wantErr: `project host "prod" mappings`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			configHome := t.TempDir()
			projectRoot := t.TempDir()
			t.Setenv("XDG_CONFIG_HOME", configHome)

			configPath := tt.configPath(configHome, projectRoot)
			if err := os.MkdirAll(filepath.Dir(configPath), 0o755); err != nil {
				t.Fatalf("MkdirAll returned error: %v", err)
			}
			if err := os.WriteFile(configPath, []byte(tt.content), 0o600); err != nil {
				t.Fatalf("WriteFile returned error: %v", err)
			}

			_, err := Load(projectRoot, "shop")
			if err == nil {
				t.Fatalf("Load returned nil, want error containing %q", tt.wantErr)
			}
			if !strings.Contains(err.Error(), tt.wantErr) {
				t.Fatalf("Load error = %q, want substring %q", err, tt.wantErr)
			}
		})
	}
}
