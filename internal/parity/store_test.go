package parity

import (
	"bufio"
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
)

func probe(t *testing.T) string {
	t.Helper()
	path := os.Getenv("DRIFT_RUST_STORE_PROBE")
	if path == "" {
		t.Skip("set DRIFT_RUST_STORE_PROBE to the built Rust store_probe example for cross-process parity")
	}
	absolute, err := filepath.Abs(path)
	if err != nil {
		t.Fatal(err)
	}
	return absolute
}
func TestGoRustGoStoredRecords(t *testing.T) {
	binary := probe(t)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	cfg, err := config.Load(t.TempDir(), "")
	if err != nil {
		t.Fatal(err)
	}
	zero := 0
	server := config.Host{Name: "server", Hostname: "example", KeepAliveInterval: &zero, Auth: config.Auth{Type: "agent"}}
	if err := config.SaveGlobalHost(cfg, server, ""); err != nil {
		t.Fatal(err)
	}
	cfg, err = config.Load(t.TempDir(), "shop")
	if err != nil {
		t.Fatal(err)
	}
	if err := config.SaveProjectHost(cfg, config.Host{Name: "prod", Server: "server", RootPath: "/srv"}, ""); err != nil {
		t.Fatal(err)
	}
	store := project.NewStore()
	date := time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC)
	registry := &project.Registry{Projects: []project.Project{{Slug: "shop", Name: "Shop", Path: "/work/shop", CreatedAt: date, UpdatedAt: date}}}
	if err := store.Save(registry); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command(binary, config.Dir(), "roundtrip").CombinedOutput(); err != nil {
		t.Fatalf("Rust roundtrip: %v %s", err, output)
	}
	cfg, err = config.Load("/work/shop", "shop")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Hosts["prod"].KeepAliveInterval == nil || *cfg.Hosts["prod"].KeepAliveInterval != 0 {
		t.Fatal("explicit zero lost across Rust")
	}
	contents, err := os.ReadFile(filepath.Join(config.Dir(), "projects/shop.toml"))
	if err != nil {
		t.Fatal(err)
	}
	// A resolved connection must not be baked into a project link.
	if bytes := string(contents); strings.Contains(bytes, "hostname") || strings.Contains(bytes, "keep_alive_interval") {
		t.Fatalf("link gained inherited connection: %s", bytes)
	}
	loaded, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if !loaded.Find("shop").CreatedAt.Equal(date) {
		t.Fatal("TOML timestamp changed")
	}
	if err := store.Save(loaded); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command(binary, config.Dir(), "roundtrip").CombinedOutput(); err != nil {
		t.Fatalf("Rust reread: %v %s", err, output)
	}
}
func TestBothProcessesUseSameFlockAndDetectRegistryConflicts(t *testing.T) {
	binary := probe(t)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	lock, err := config.LockWrites()
	if err != nil {
		t.Fatal(err)
	}
	output, err := exec.Command(binary, config.Dir(), "host", "blocked").CombinedOutput()
	if err == nil {
		t.Fatal("Rust wrote while Go held flock")
	}
	if err := lock.Close(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(output), "another drift process") {
		t.Fatalf("unexpected lock error: %s", output)
	}
	if output, err := exec.Command(binary, config.Dir(), "host", "rust").CombinedOutput(); err != nil {
		t.Fatalf("Rust write: %v %s", err, output)
	}
	cfg, err := config.Load(t.TempDir(), "")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Hosts["rust"].Hostname != "rust.example" {
		t.Fatal("Rust host missing")
	}
	store := project.NewStore()
	date := time.Now().UTC()
	reg := &project.Registry{Projects: []project.Project{{Slug: "shop", Name: "Shop", Path: "/work/shop", CreatedAt: date, UpdatedAt: date}}}
	if err := store.Save(reg); err != nil {
		t.Fatal(err)
	}
	stale, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command(binary, config.Dir(), "rename-project").CombinedOutput(); err != nil {
		t.Fatalf("Rust project edit: %v %s", err, output)
	}
	stale.Find("shop").Name = "Go stale edit"
	var conflict *config.ConflictError
	if err := store.Save(stale); !errors.As(err, &conflict) {
		t.Fatalf("expected conflict: %v", err)
	}
	current, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if current.Find("shop").Name != "Rust edit" {
		t.Fatal("Rust edit overwritten")
	}
}

func TestGoCannotWriteWhileRustHoldsFlock(t *testing.T) {
	binary := probe(t)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, binary, config.Dir(), "hold")
	input, err := command.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	output, err := command.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := command.Start(); err != nil {
		t.Fatal(err)
	}
	defer input.Close()
	ready, err := bufio.NewReader(output).ReadString('\n')
	if err != nil || ready != "locked\n" {
		t.Fatalf("Rust lock readiness: %q %v", ready, err)
	}
	cfg := &config.MergedConfig{}
	if err := config.SaveGlobalHost(cfg, config.Host{Name: "blocked"}, ""); !errors.Is(err, config.ErrWriteBusy) {
		t.Fatalf("Go wrote through Rust lock: %v", err)
	}
	certificate := config.TrustedCertificate{Protocol: "ftps", Hostname: "locked.example", Port: 21, Fingerprint: strings.TrimSuffix(strings.Repeat("AB:", 32), ":"), Problems: []string{"unknown_authority"}, TrustedAt: time.Now().UTC()}
	if err := config.SaveTrustedCertificate(certificate); !errors.Is(err, config.ErrWriteBusy) {
		t.Fatalf("Go trust save wrote through Rust lock: %v", err)
	}
	if err := config.DeleteTrustedCertificate("ftps", "locked.example", 21); !errors.Is(err, config.ErrWriteBusy) {
		t.Fatalf("Go trust delete wrote through Rust lock: %v", err)
	}
	if _, err := input.Write([]byte("x")); err != nil {
		t.Fatal(err)
	}
	if err := command.Wait(); err != nil {
		t.Fatal(err)
	}
	if err := config.SaveGlobalHost(cfg, config.Host{Name: "allowed"}, ""); err != nil {
		t.Fatal(err)
	}
}

func TestCertificateTrustRoundtripAndSharedFlock(t *testing.T) {
	binary := probe(t)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	date := time.Date(2026, 1, 2, 3, 4, 5, 123456789, time.FixedZone("test", 3600))
	original := config.TrustedCertificate{Protocol: "ftps", Hostname: "go.example", Port: 2121, Fingerprint: strings.TrimSuffix(strings.Repeat("CD:", 32), ":"), Problems: []string{"expired", "unknown_authority"}, TrustedAt: date}
	if err := config.SaveTrustedCertificate(original); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command(binary, config.Dir(), "roundtrip").CombinedOutput(); err != nil {
		t.Fatalf("Go→Rust trust: %v %s", err, output)
	}
	entries, err := config.LoadTrustedCertificates()
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 1 || entries[0].Fingerprint != original.Fingerprint || !entries[0].TrustedAt.Equal(date) || strings.Join(entries[0].Problems, ",") != strings.Join(original.Problems, ",") {
		t.Fatalf("trust record changed: %+v", entries)
	}
	lock, err := config.LockWrites()
	if err != nil {
		t.Fatal(err)
	}
	output, writeErr := exec.Command(binary, config.Dir(), "trust", "rust.example").CombinedOutput()
	if err = lock.Close(); err != nil {
		t.Fatal(err)
	}
	if writeErr == nil || !strings.Contains(string(output), "another drift process") {
		t.Fatalf("Rust wrote trust through Go lock: %v %s", writeErr, output)
	}
	if output, err = exec.Command(binary, config.Dir(), "trust", "rust.example").CombinedOutput(); err != nil {
		t.Fatalf("Rust→Go trust: %v %s", err, output)
	}
	entries, err = config.LoadTrustedCertificates()
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 || entries[1].Hostname != "rust.example" || len(entries[1].Problems) != 2 {
		t.Fatalf("lost concurrent endpoint: %+v", entries)
	}
	if err = config.SaveTrustedCertificate(entries[1]); err != nil {
		t.Fatal(err)
	}
	if output, err = exec.Command(binary, config.Dir(), "roundtrip").CombinedOutput(); err != nil {
		t.Fatalf("Rust→Go→Rust trust: %v %s", err, output)
	}
	info, err := os.Stat(filepath.Join(config.Dir(), "trusted-certificates.toml"))
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0600 {
		t.Fatalf("trust permissions: %v", info.Mode())
	}
}
