package tui

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
)

func makeProjectDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, ".drift"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".drift", "config.toml"), []byte("# project\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	return dir
}

func TestShouldPromptRegister(t *testing.T) {
	dir := makeProjectDir(t)

	cfg := &config.MergedConfig{ProjectRoot: dir}
	emptyReg := &project.Registry{}

	if !shouldPromptRegister(dir, cfg, emptyReg) {
		t.Fatal("unregistered project should prompt")
	}

	registered := &project.Registry{Projects: []project.Project{{Slug: "x", Path: dir}}}
	if shouldPromptRegister(dir, cfg, registered) {
		t.Fatal("registered project should not prompt")
	}

	// A directory without .drift must never prompt.
	plain := t.TempDir()
	if shouldPromptRegister(plain, &config.MergedConfig{ProjectRoot: plain}, emptyReg) {
		t.Fatal("non-project dir should not prompt")
	}

	// Nil registry/config guards.
	if shouldPromptRegister(dir, nil, emptyReg) || shouldPromptRegister(dir, cfg, nil) {
		t.Fatal("nil cfg/reg should not prompt")
	}
}
