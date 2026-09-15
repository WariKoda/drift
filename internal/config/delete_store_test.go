package config

import (
	"errors"
	"os"
	"testing"
)

func TestDeleteProjectStoreRemovesProjectSettings(t *testing.T) {
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

	if err := DeleteProjectStore("shop"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("project store still exists: %v", err)
	}
	if err := DeleteProjectStore("shop"); err != nil {
		t.Fatalf("deleting a missing store: %v", err)
	}
}
