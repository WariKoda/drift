package log

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestInitWritesToFile(t *testing.T) {
	// Reset package state after the test so other tests see a clean slate.
	t.Cleanup(func() { logger = nil; enabled = false })

	path := filepath.Join(t.TempDir(), "sub", "drift.log")
	closer, err := Init(Options{Path: path})
	if err != nil {
		t.Fatalf("Init: %v", err)
	}
	if !Enabled() {
		t.Fatal("Enabled() = false after Init")
	}

	Info("hello", "k", "v")
	Error("boom", "err", "nope")
	if err := closer.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	out := string(data)
	if !strings.Contains(out, "hello") || !strings.Contains(out, "k=v") {
		t.Errorf("info line missing: %q", out)
	}
	if !strings.Contains(out, "boom") {
		t.Errorf("error line missing: %q", out)
	}
}

func TestHelpersNoOpWhenDisabled(t *testing.T) {
	logger = nil
	enabled = false
	if Enabled() {
		t.Fatal("Enabled() = true before Init")
	}
	// Must not panic despite logger being nil.
	Info("ignored")
	Error("ignored")
	Debug("ignored")
}
