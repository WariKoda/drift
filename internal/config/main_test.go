package config

import (
	"os"
	"testing"
)

// TestMain points $XDG_CONFIG_HOME at a temporary directory for every test in
// this package.
//
// The writers reach the config directory now that a project host's access is
// stored there, so a test that calls SaveProjectHost without isolating the
// environment writes into the developer's own ~/.config/drift — which is how
// stray entries from test runs ended up in a real access.toml. Individual
// tests may still override this with t.Setenv; nothing may rely on the real
// directory.
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
