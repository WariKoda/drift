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

func TestConcurrentRegistryChangesMergeDifferentRecordsAndConflictOnSameRecord(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store := NewStore()
	initial := &Registry{Projects: []Project{{Slug: "a", Name: "A", Path: "/work/a"}, {Slug: "b", Name: "B", Path: "/work/b"}}}
	if err := store.Save(initial); err != nil {
		t.Fatal(err)
	}
	first, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	second, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	first.Find("a").Name = "A edited"
	second.Find("b").Name = "B edited"
	if err := store.Save(first); err != nil {
		t.Fatal(err)
	}
	if err := store.Save(second); err != nil {
		t.Fatal(err)
	}
	if second.Find("a").Name != "A edited" || second.Find("b").Name != "B edited" {
		t.Fatal("unrelated changes lost")
	}
	first.Find("b").Name = "stale edit"
	if err := store.Save(first); err == nil {
		t.Fatal("stale record overwritten")
	}
}

func TestRegistrySaveComparesPersistedTimestampInstants(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store := NewStore()
	date := time.Now() // includes process-local monotonic metadata
	reg := &Registry{Projects: []Project{{Slug: "clock", Name: "Clock", Path: t.TempDir(), CreatedAt: date, UpdatedAt: date}}}
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	loaded, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if !reg.Projects[0].Equal(loaded.Projects[0]) {
		t.Fatal("persistence changed the timestamp instant")
	}
	reg.Find("clock").Name = "Clock edited"
	if err := store.Save(reg); err != nil {
		t.Fatalf("monotonic clock metadata caused a false conflict: %v", err)
	}
}
