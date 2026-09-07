package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLegacyInsecureTLSIsIgnored(t *testing.T) {
	isolate(t)
	if err := os.MkdirAll(filepath.Dir(globalConfigPath()), 0o700); err != nil {
		t.Fatal(err)
	}
	content := `[[hosts]]
name = "legacy"
hostname = "legacy.example"
port = 21
protocol = "ftps"
insecure_tls = true
root_path = "/"
`
	if err := os.WriteFile(globalConfigPath(), []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	cfg, err := Load(t.TempDir(), "")
	if err != nil {
		t.Fatalf("load legacy config: %v", err)
	}
	host, ok := cfg.Hosts["legacy"]
	if !ok || host.Protocol != "ftps" {
		t.Fatalf("legacy host was not otherwise loaded: %+v", host)
	}
	entries, err := LoadTrustedCertificates()
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("legacy insecure_tls granted trust: %+v", entries)
	}
}
