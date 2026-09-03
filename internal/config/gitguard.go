package config

import (
	"os/exec"
	"path/filepath"
	"strings"
)

// Exposure describes how visible .drift/config.toml is to Git.
type Exposure int

const (
	// ExposureSafe means the file cannot reach a commit: either projectRoot is
	// not inside a Git work tree, or the file is ignored.
	ExposureSafe Exposure = iota
	// ExposureUntracked means the file sits in a work tree without being
	// ignored, so `git add .` would stage it.
	ExposureUntracked
	// ExposureTracked means Git already tracks the file — credentials are in
	// the index and possibly in pushed history.
	ExposureTracked
)

// projectConfigRel is the path of the project config relative to the project root.
var projectConfigRel = filepath.Join(".drift", "config.toml")

// ProjectConfigExposure reports whether Git can pick up the project config.
// A missing git binary, a directory outside any work tree, or any git failure
// all report ExposureSafe: the check exists to warn, never to block a save.
func ProjectConfigExposure(projectRoot string) Exposure {
	if projectRoot == "" {
		return ExposureSafe
	}
	if out, err := git(projectRoot, "rev-parse", "--is-inside-work-tree"); err != nil || out != "true" {
		return ExposureSafe
	}
	if _, err := git(projectRoot, "ls-files", "--error-unmatch", "--", projectConfigRel); err == nil {
		return ExposureTracked
	}
	if _, err := git(projectRoot, "check-ignore", "-q", "--", projectConfigRel); err == nil {
		return ExposureSafe
	}
	return ExposureUntracked
}

// HasPlaintextSecret reports whether the host stores a literal credential.
// An $ENV reference is resolved at connect time and leaks nothing on its own,
// and key file and agent auth keep the secret outside the config entirely.
func (h Host) HasPlaintextSecret() bool {
	return isLiteralSecret(h.Auth.Password) || isLiteralSecret(h.Auth.Passphrase)
}

func isLiteralSecret(v string) bool {
	v = strings.TrimSpace(v)
	return v != "" && !strings.HasPrefix(v, "$")
}

// git runs a git command in dir and returns its trimmed stdout.
func git(dir string, args ...string) (string, error) {
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}
