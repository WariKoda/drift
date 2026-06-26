package cmd

import (
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/config"
)

func TestResolveLogConfig(t *testing.T) {
	defaultPath := filepath.Join(config.Dir(), "drift.log")

	tests := []struct {
		name        string
		flagLog     string
		flagDebug   bool
		envLog      string
		envDebug    string
		wantEnabled bool
		wantPath    string
		wantDebug   bool
	}{
		{"all off", "", false, "", "", false, "", false},
		{"env debug → default path", "", false, "", "1", true, defaultPath, true},
		{"env log → info level", "", false, "/tmp/a.log", "", true, "/tmp/a.log", false},
		{"flag log wins over env", "/tmp/b.log", false, "/tmp/a.log", "", true, "/tmp/b.log", false},
		{"flag debug + flag log", "/tmp/b.log", true, "", "", true, "/tmp/b.log", true},
		{"env debug false stays off", "", false, "", "false", false, "", false},
		{"env debug 0 stays off", "", false, "", "0", false, "", false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			flagLog = tt.flagLog
			flagDebug = tt.flagDebug
			t.Cleanup(func() { flagLog = ""; flagDebug = false })
			t.Setenv("DRIFT_LOG", tt.envLog)
			t.Setenv("DRIFT_DEBUG", tt.envDebug)

			opts, enabled := resolveLogConfig()
			if enabled != tt.wantEnabled {
				t.Fatalf("enabled = %v, want %v", enabled, tt.wantEnabled)
			}
			if !enabled {
				return
			}
			if opts.Path != tt.wantPath {
				t.Errorf("path = %q, want %q", opts.Path, tt.wantPath)
			}
			if opts.Debug != tt.wantDebug {
				t.Errorf("debug = %v, want %v", opts.Debug, tt.wantDebug)
			}
		})
	}
}

func TestShouldShowDashboard(t *testing.T) {
	tests := []struct {
		name          string
		dash, noDash  bool
		hasProjectCtx bool
		regCount      int
		want          bool
	}{
		{"no flags, outside project, has projects", false, false, false, 2, true},
		{"no flags, outside project, no projects", false, false, false, 0, false},
		{"no flags, inside project", false, false, true, 5, false},
		{"force dashboard inside project", true, false, true, 0, true},
		{"force dashboard with no projects", true, false, false, 0, true},
		{"no-dashboard wins over context", false, true, false, 3, false},
		{"no-dashboard wins over dashboard flag", true, true, false, 3, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := shouldShowDashboard(tt.dash, tt.noDash, tt.hasProjectCtx, tt.regCount); got != tt.want {
				t.Fatalf("shouldShowDashboard(%v,%v,%v,%d) = %v, want %v",
					tt.dash, tt.noDash, tt.hasProjectCtx, tt.regCount, got, tt.want)
			}
		})
	}
}
