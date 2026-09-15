package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestMergeDefaultsPortPerProtocol(t *testing.T) {
	global := &GlobalConfig{Hosts: []Host{
		{Name: "sftp-host"},
		{Name: "implicit-sftp", Protocol: ""},
		{Name: "ftp-host", Protocol: "ftp"},
		{Name: "ftps-host", Protocol: "ftps"},
	}}

	merged := merge(global, nil, "")

	want := map[string]int{
		"sftp-host":     22,
		"implicit-sftp": 22,
		"ftp-host":      21,
		"ftps-host":     21,
	}
	for name, port := range want {
		if got := merged.Hosts[name].Port; got != port {
			t.Errorf("host %q port = %d, want %d", name, got, port)
		}
	}
}

func TestMergeDefaultsPortPrefersConfiguredDefault(t *testing.T) {
	global := &GlobalConfig{
		Defaults: Defaults{Port: 2222},
		Hosts:    []Host{{Name: "ftp-host", Protocol: "ftp"}},
	}

	merged := merge(global, nil, "")

	if got := merged.Hosts["ftp-host"].Port; got != 2222 {
		t.Errorf("host port = %d, want the configured default 2222", got)
	}
}

func TestLoadGlobalKeepsFtpPortWithoutDefaults(t *testing.T) {
	isolate(t)

	dir := Dir()
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	content := "[[hosts]]\nname = \"web\"\nhostname = \"ftp.example.com\"\nprotocol = \"ftp\"\n"
	if err := os.WriteFile(filepath.Join(dir, "config.toml"), []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}

	cfg, err := Load("", "")
	if err != nil {
		t.Fatal(err)
	}

	if got := cfg.Hosts["web"].Port; got != 21 {
		t.Errorf("port = %d, want 21 for an ftp host in a config without [defaults]", got)
	}
}
