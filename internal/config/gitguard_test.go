package config

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// initRepo creates a real Git repository in a temp dir, skipping the test when
// no git binary is available.
func initRepo(t *testing.T) string {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git binary not available")
	}
	dir := t.TempDir()
	for _, args := range [][]string{
		{"init"},
		{"config", "user.email", "test@example.com"},
		{"config", "user.name", "test"},
	} {
		cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v failed: %v\n%s", args, err, out)
		}
	}
	return dir
}

func TestProjectConfigExposureOutsideRepo(t *testing.T) {
	dir := t.TempDir()
	cfg := &MergedConfig{ProjectRoot: dir}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Hostname: "example.com"}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	if got := ProjectConfigExposure(dir); got != ExposureSafe {
		t.Fatalf("exposure outside a work tree = %v, want ExposureSafe", got)
	}
}

func TestProjectConfigExposureIgnoredByWrittenGitignore(t *testing.T) {
	dir := initRepo(t)
	cfg := &MergedConfig{ProjectRoot: dir}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Hostname: "example.com"}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}

	data, err := os.ReadFile(filepath.Join(dir, ".drift", ".gitignore"))
	if err != nil {
		t.Fatalf("drift did not write .drift/.gitignore: %v", err)
	}
	if got := string(data); got != projectGitignore {
		t.Fatalf("unexpected .gitignore content: %q", got)
	}
	if got := ProjectConfigExposure(dir); got != ExposureSafe {
		t.Fatalf("exposure with drift's own .gitignore = %v, want ExposureSafe", got)
	}
}

func TestProjectConfigExposureUntracked(t *testing.T) {
	dir := initRepo(t)
	// A pre-existing .gitignore is left alone, so the config stays visible.
	if err := os.MkdirAll(filepath.Join(dir, ".drift"), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".drift", ".gitignore"), []byte("*.log\n"), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
	cfg := &MergedConfig{ProjectRoot: dir}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Hostname: "example.com"}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	if got := ProjectConfigExposure(dir); got != ExposureUntracked {
		t.Fatalf("exposure of an unignored config = %v, want ExposureUntracked", got)
	}
}

func TestProjectConfigExposureTracked(t *testing.T) {
	dir := initRepo(t)
	cfg := &MergedConfig{ProjectRoot: dir}
	if err := SaveProjectHost(cfg, Host{Name: "prod", Hostname: "example.com"}, ""); err != nil {
		t.Fatalf("SaveProjectHost returned error: %v", err)
	}
	// Simulate a config that was committed before drift wrote the ignore file.
	add := exec.Command("git", "-C", dir, "add", "-f", "--", projectConfigRel)
	if out, err := add.CombinedOutput(); err != nil {
		t.Fatalf("git add failed: %v\n%s", err, out)
	}
	if got := ProjectConfigExposure(dir); got != ExposureTracked {
		t.Fatalf("exposure of a tracked config = %v, want ExposureTracked", got)
	}
}

func TestHasPlaintextSecret(t *testing.T) {
	cases := []struct {
		name string
		auth Auth
		want bool
	}{
		{"agent auth", Auth{Type: "agent"}, false},
		{"env password", Auth{Type: "password", Password: "$DEPLOY_PW"}, false},
		{"literal password", Auth{Type: "password", Password: "hunter2"}, true},
		{"key file only", Auth{Type: "keyfile", KeyFile: "~/.ssh/id_ed25519"}, false},
		{"literal passphrase", Auth{Type: "keyfile", KeyFile: "~/.ssh/id_ed25519", Passphrase: "s3cret"}, true},
		{"env passphrase", Auth{Type: "keyfile", Passphrase: "$KEY_PASS"}, false},
		{"whitespace only", Auth{Type: "password", Password: "  "}, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := (Host{Auth: tc.auth}).HasPlaintextSecret(); got != tc.want {
				t.Fatalf("HasPlaintextSecret() = %v, want %v", got, tc.want)
			}
		})
	}
}
