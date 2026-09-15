package config

import (
	"errors"
	"os"
	"testing"
)

func TestRemoveProjectStoreRemovesProjectSettingsAfterCommit(t *testing.T) {
	isolate(t)
	if err := writeProjectStore("shop", ProjectConfig{
		Hosts: []Host{{Name: "prod", Hostname: "example.com"}},
	}); err != nil {
		t.Fatal(err)
	}
	path, err := projectStorePath("shop")
	if err != nil {
		t.Fatal(err)
	}

	committed := false
	if err := RemoveProjectStore("shop", func() error {
		committed = true
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if !committed {
		t.Fatal("registry commit was not called")
	}
	if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("project store still exists: %v", err)
	}
	if err := RemoveProjectStore("shop", func() error { return nil }); err != nil {
		t.Fatalf("deleting a missing store: %v", err)
	}
}

func TestRemoveProjectStoreRestoresSettingsAfterFailedCommit(t *testing.T) {
	isolate(t)
	want := ProjectConfig{Hosts: []Host{{Name: "prod", Hostname: "example.com"}}}
	if err := writeProjectStore("shop", want); err != nil {
		t.Fatal(err)
	}
	commitErr := errors.New("registry write failed")
	if err := RemoveProjectStore("shop", func() error { return commitErr }); !errors.Is(err, commitErr) {
		t.Fatalf("RemoveProjectStore error = %v, want commit error", err)
	}
	stored, err := loadProjectStore("shop")
	if err != nil || stored == nil || len(stored.Hosts) != 1 || stored.Hosts[0].Name != "prod" {
		t.Fatalf("project store was not restored: %#v, %v", stored, err)
	}
}
