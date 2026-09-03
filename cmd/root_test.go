package cmd

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
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

func TestResolveStart(t *testing.T) {
	last := &project.Project{Slug: "recent", Path: "/tmp/recent"}
	tests := []struct {
		name          string
		dash, noDash  bool
		hasProjectCtx bool
		last          *project.Project
		regCount      int
		want          startMode
	}{
		{"outside + last opened", false, false, false, last, 2, startLastProject},
		{"outside + projects none opened", false, false, false, nil, 2, startDashboard},
		{"outside + no projects", false, false, false, nil, 0, startBrowserCwd},
		{"inside project", false, false, true, last, 5, startBrowserCwd},
		{"dashboard flag with last opened", true, false, false, last, 2, startDashboard},
		{"dashboard flag inside project", true, false, true, last, 0, startDashboard},
		{"no-dashboard with last opened", false, true, false, last, 3, startBrowserCwd},
		{"no-dashboard wins over dashboard flag", true, true, false, last, 3, startBrowserCwd},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := resolveStart(tt.dash, tt.noDash, tt.hasProjectCtx, tt.last, tt.regCount)
			if got != tt.want {
				t.Fatalf("resolveStart(...) = %d, want %d", got, tt.want)
			}
		})
	}
}

func TestUsableLastProject(t *testing.T) {
	dir := t.TempDir()
	now := time.Now().UTC()

	ok := &project.Registry{Projects: []project.Project{
		{Slug: "ok", Path: dir, OpenedAt: now},
	}}
	if p := usableLastProject(ok); p == nil || p.Slug != "ok" {
		t.Fatalf("usable dir: got %v", p)
	}

	missing := &project.Registry{Projects: []project.Project{
		{Slug: "gone", Path: filepath.Join(dir, "nope"), OpenedAt: now},
	}}
	if p := usableLastProject(missing); p != nil {
		t.Fatalf("missing path: got %v", p)
	}

	file := filepath.Join(dir, "not-a-dir")
	if err := os.WriteFile(file, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	asFile := &project.Registry{Projects: []project.Project{
		{Slug: "file", Path: file, OpenedAt: now},
	}}
	if p := usableLastProject(asFile); p != nil {
		t.Fatalf("file path: got %v", p)
	}

	never := &project.Registry{Projects: []project.Project{
		{Slug: "n", Path: dir},
	}}
	if p := usableLastProject(never); p != nil {
		t.Fatalf("never opened: got %v", p)
	}
}

func TestWorthStayingIn(t *testing.T) {
	empty := &project.Registry{}

	// Inside an open project: stay, obviously.
	if !worthStayingIn(t.TempDir(), &config.MergedConfig{ProjectSlug: "shop"}, empty) {
		t.Fatal("an open project is not worth staying in")
	}

	// A repository drift could register: stay, or the offer never appears.
	repo := t.TempDir()
	if err := os.MkdirAll(filepath.Join(repo, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if !worthStayingIn(repo, &config.MergedConfig{}, empty) {
		t.Fatal("an unregistered repository is not worth staying in")
	}
	registered := &project.Registry{Projects: []project.Project{{Slug: "x", Path: repo}}}
	if worthStayingIn(repo, &config.MergedConfig{}, registered) {
		t.Fatal("a repository that is registered but not open kept drift in the directory")
	}

	// Nothing here: let the last project or the dashboard win.
	if worthStayingIn(t.TempDir(), &config.MergedConfig{}, empty) {
		t.Fatal("a plain directory kept drift in the directory")
	}
}
