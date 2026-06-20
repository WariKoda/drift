package project

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestStoreSaveLoadRoundTrip(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	s := NewStore()

	created := time.Date(2026, 6, 20, 10, 0, 0, 0, time.UTC)
	reg := &Registry{Projects: []Project{{
		Slug:      "kunde-a",
		Name:      "KUNDE A",
		Path:      "/home/nibra/work/kunde-a",
		CreatedAt: created,
		UpdatedAt: created,
	}}}

	if err := s.Save(reg); err != nil {
		t.Fatalf("Save: %v", err)
	}

	loaded, err := s.Load()
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(loaded.Projects) != 1 {
		t.Fatalf("loaded %d projects, want 1", len(loaded.Projects))
	}
	got := loaded.Projects[0]
	if got.Slug != "kunde-a" || got.Name != "KUNDE A" || got.Path != "/home/nibra/work/kunde-a" {
		t.Fatalf("round-trip mismatch: %+v", got)
	}
	if !got.CreatedAt.Equal(created) {
		t.Fatalf("CreatedAt mismatch: %v", got.CreatedAt)
	}
}

func TestStoreLoadMissingReturnsEmpty(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	s := NewStore()
	reg, err := s.Load()
	if err != nil {
		t.Fatalf("Load missing file: unexpected error %v", err)
	}
	if len(reg.Projects) != 0 {
		t.Fatalf("expected empty registry, got %d", len(reg.Projects))
	}
}

func TestStoreLoadCorruptErrors(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	if err := os.MkdirAll(filepath.Join(dir, "drift"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "drift", "projects.toml"), []byte("this = = broken"), 0o600); err != nil {
		t.Fatal(err)
	}
	s := NewStore()
	if _, err := s.Load(); err == nil {
		t.Fatal("expected error loading corrupt projects.toml")
	}
}

func TestStorePathUsesConfigDir(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	want := filepath.Join(dir, "drift", "projects.toml")
	if got := NewStore().Path(); got != want {
		t.Fatalf("Path() = %q, want %q", got, want)
	}
}
