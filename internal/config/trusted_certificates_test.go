package config

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

const testFingerprint = "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA"

func TestTrustedCertificatesRoundTripAndReplace(t *testing.T) {
	isolate(t)
	first := TrustedCertificate{
		Protocol:    "ftps",
		Hostname:    "one.example",
		Port:        21,
		Fingerprint: testFingerprint,
		Problems:    []string{"unknown_authority"},
		TrustedAt:   time.Date(2026, 2, 1, 12, 0, 0, 0, time.UTC),
	}
	if err := SaveTrustedCertificate(first); err != nil {
		t.Fatalf("save first trust: %v", err)
	}
	path := filepath.Join(Dir(), "trusted-certificates.toml")
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat trust store: %v", err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %o, want 600", info.Mode().Perm())
	}

	replacement := first
	replacement.Fingerprint = "BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB:BB"
	replacement.Problems = []string{"expired", "unknown_authority"}
	if err := SaveTrustedCertificate(replacement); err != nil {
		t.Fatalf("replace trust: %v", err)
	}

	entries, err := LoadTrustedCertificates()
	if err != nil {
		t.Fatalf("load trust: %v", err)
	}
	if len(entries) != 1 || entries[0].Fingerprint != replacement.Fingerprint {
		t.Fatalf("entries = %+v", entries)
	}
	if err := DeleteTrustedCertificate("ftps", "one.example", 21); err != nil {
		t.Fatalf("delete trust: %v", err)
	}
	entries, err = LoadTrustedCertificates()
	if err != nil {
		t.Fatalf("load after delete: %v", err)
	}
	if len(entries) != 0 {
		t.Fatalf("entries after delete = %+v", entries)
	}
}

func TestTrustedCertificatesRejectInvalidAndDuplicateEntries(t *testing.T) {
	isolate(t)
	if err := SaveTrustedCertificate(TrustedCertificate{
		Protocol: "ftps", Hostname: "one.example", Port: 21,
		Fingerprint: testFingerprint, Problems: []string{"future_problem"}, TrustedAt: time.Now(),
	}); err == nil {
		t.Fatal("invalid problem was accepted")
	}

	if err := os.MkdirAll(Dir(), 0o700); err != nil {
		t.Fatal(err)
	}
	content := `[[certificates]]
protocol = "ftps"
hostname = "one.example"
port = 21
fingerprint = "` + testFingerprint + `"
problems = ["unknown_authority"]
trusted_at = 2026-02-01T12:00:00Z

[[certificates]]
protocol = "ftps"
hostname = "one.example"
port = 21
fingerprint = "` + testFingerprint + `"
problems = ["unknown_authority"]
trusted_at = 2026-02-01T12:00:00Z
`
	if err := os.WriteFile(trustedCertificatesPath(), []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadTrustedCertificates(); err == nil {
		t.Fatal("duplicate endpoint was accepted")
	}
}

func TestTrustedCertificateConcurrentSavesPreserveEntries(t *testing.T) {
	isolate(t)
	const count = 8
	var wg sync.WaitGroup
	errs := make(chan error, count)
	for i := 0; i < count; i++ {
		wg.Add(1)
		go func(index int) {
			defer wg.Done()
			errs <- SaveTrustedCertificate(TrustedCertificate{
				Protocol: "ftps", Hostname: fmt.Sprintf("host-%d.example", index), Port: 21,
				Fingerprint: testFingerprint, Problems: []string{"unknown_authority"}, TrustedAt: time.Now(),
			})
		}(i)
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatalf("concurrent save: %v", err)
		}
	}
	entries, err := LoadTrustedCertificates()
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != count {
		t.Fatalf("entry count = %d, want %d", len(entries), count)
	}
}
