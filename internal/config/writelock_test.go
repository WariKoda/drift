package config

import (
	"errors"
	"testing"
	"time"
)

func TestWriteLockCoordinatesAllManagementPaths(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{ProjectSlug: "shop"}
	lock, err := LockWrites()
	if err != nil {
		t.Fatal(err)
	}
	for name, operation := range map[string]func() error{
		"global":              func() error { return SaveGlobalHost(cfg, Host{Name: "server"}, "") },
		"project":             func() error { return SaveProjectHost(cfg, Host{Name: "host"}, "") },
		"delete global host":  func() error { return DeleteGlobalHost(cfg, "server") },
		"delete project host": func() error { return DeleteProjectHost(cfg, "host") },
		"promote":             func() error { _, err := PromoteProjectHost(cfg, "other", "host"); return err },
		"save trust": func() error {
			return SaveTrustedCertificate(TrustedCertificate{
				Protocol: "ftps", Hostname: "example", Port: 21,
				Fingerprint: testFingerprint, Problems: []string{"expired"}, TrustedAt: time.Now(),
			})
		},
		"delete trust": func() error { return DeleteTrustedCertificate("ftps", "example", 21) },
		"delete project": func() error {
			return RemoveProjectStore("shop", func(_ *WriteLock) error { t.Fatal("commit ran without lock"); return nil })
		},
	} {
		if err := operation(); !errors.Is(err, ErrWriteBusy) {
			t.Fatalf("%s: expected busy, got %v", name, err)
		}
	}
	if err := lock.Close(); err != nil {
		t.Fatal(err)
	}
	if err := SaveGlobalHost(cfg, Host{Name: "server"}, ""); err != nil {
		t.Fatal(err)
	}
}
func TestHostConflictPreservesExternallyChangedRecord(t *testing.T) {
	isolate(t)
	cfg := &MergedConfig{}
	original := Host{Name: "prod", Hostname: "old.example"}
	if err := SaveGlobalHost(cfg, original, ""); err != nil {
		t.Fatal(err)
	}
	other, err := Load(t.TempDir(), "")
	if err != nil {
		t.Fatal(err)
	}
	external := original
	external.Hostname = "new.example"
	if err := SaveGlobalHost(other, external, "prod"); err != nil {
		t.Fatal(err)
	}
	var conflict *ConflictError
	if err := SaveGlobalHost(cfg, original, "prod"); !errors.As(err, &conflict) {
		t.Fatalf("expected conflict, got %v", err)
	}
	stored, err := loadGlobal()
	if err != nil || stored.Hosts[0].Hostname != "new.example" {
		t.Fatalf("external change overwritten: %+v %v", stored, err)
	}
}
