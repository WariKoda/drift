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

// writeLegacyProjectConfig puts a pre-store .drift/config.toml in place, the
// only file ProjectConfigExposure still asks git about.
func writeLegacyProjectConfig(t *testing.T, root string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(root, ".drift"), 0o700); err != nil {
		t.Fatalf("MkdirAll returned error: %v", err)
	}
	body := "[[hosts]]\n  name = \"prod\"\n  hostname = \"example.com\"\n"
	if err := os.WriteFile(filepath.Join(root, ".drift", "config.toml"), []byte(body), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
}

func TestProjectConfigExposureOutsideRepo(t *testing.T) {
	isolate(t)
	dir := t.TempDir()
	writeLegacyProjectConfig(t, dir)
	if got := ProjectConfigExposure(dir); got != ExposureSafe {
		t.Fatalf("exposure outside a work tree = %v, want ExposureSafe", got)
	}
}

func TestProjectConfigExposureIgnoredByWrittenGitignore(t *testing.T) {
	isolate(t)
	dir := initRepo(t)
	writeLegacyProjectConfig(t, dir)
	if err := os.WriteFile(filepath.Join(dir, ".drift", ".gitignore"), []byte(projectGitignore), 0o600); err != nil {
		t.Fatalf("WriteFile returned error: %v", err)
	}
	if got := ProjectConfigExposure(dir); got != ExposureSafe {
		t.Fatalf("exposure of an ignored config = %v, want ExposureSafe", got)
	}
}

func TestProjectConfigExposureUntracked(t *testing.T) {
	isolate(t)
	dir := initRepo(t)
	writeLegacyProjectConfig(t, dir)
	if got := ProjectConfigExposure(dir); got != ExposureUntracked {
		t.Fatalf("exposure of an unignored config = %v, want ExposureUntracked", got)
	}
}

func TestProjectConfigExposureTracked(t *testing.T) {
	isolate(t)
	dir := initRepo(t)
	writeLegacyProjectConfig(t, dir)
	// A config that was committed before drift stopped writing into projects.
	add := exec.Command("git", "-C", dir, "add", "-f", "--", projectConfigRel)
	if out, err := add.CombinedOutput(); err != nil {
		t.Fatalf("git add failed: %v\n%s", err, out)
	}
	if got := ProjectConfigExposure(dir); got != ExposureTracked {
		t.Fatalf("exposure of a tracked config = %v, want ExposureTracked", got)
	}
}

func TestHasPlaintextSecret(t *testing.T) {
	isolate(t)
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
