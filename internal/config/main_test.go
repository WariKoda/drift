package config

import (
	"os"
	"testing"
)

// isolate gives one test its own config directory. TestMain keeps the suite
// away from the developer's real one; this keeps tests away from each other,
// since the per-project store is keyed by slug and two tests using the same
// slug would otherwise share a file.
func isolate(t *testing.T) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
}

// TestMain points $XDG_CONFIG_HOME at a temporary directory for every test in
// this package. Individual tests may still override this with t.Setenv;
// nothing may rely on the real directory.
func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "drift-config-test-*")
	if err != nil {
		panic(err)
	}
	if err := os.Setenv("XDG_CONFIG_HOME", dir); err != nil {
		panic(err)
	}
	code := m.Run()
	os.RemoveAll(dir)
	os.Exit(code)
}
