package project

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// ExpandPath turns a user-supplied path into an absolute one: it expands a
// leading ~ to the home directory and resolves relative paths against the
// current working directory.
func ExpandPath(p string) (string, error) {
	p = strings.TrimSpace(p)
	if p == "" {
		return "", fmt.Errorf("path must not be empty")
	}
	if p == "~" || strings.HasPrefix(p, "~/") {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		if p == "~" {
			p = home
		} else {
			p = filepath.Join(home, p[2:])
		}
	}
	abs, err := filepath.Abs(p)
	if err != nil {
		return "", err
	}
	return abs, nil
}

// GitRoot walks up from dir looking for a .git entry and returns the directory
// holding it.
//
// A project usually is a repository, so this is what drift suggests as the
// project path: it beats the working directory, which is often some
// subdirectory the user happened to be in. .git is matched as a file too, since
// that is how a worktree or submodule links to its repository.
func GitRoot(dir string) (string, bool) {
	current, err := filepath.Abs(dir)
	if err != nil {
		return "", false
	}
	for {
		if _, err := os.Lstat(filepath.Join(current, ".git")); err == nil {
			return current, true
		}
		parent := filepath.Dir(current)
		if parent == current {
			return "", false
		}
		current = parent
	}
}

// SuggestRoot is the directory drift proposes as a project path for dir: its
// repository root when there is one, otherwise dir itself.
func SuggestRoot(dir string) string {
	if root, ok := GitRoot(dir); ok {
		return root
	}
	abs, err := filepath.Abs(dir)
	if err != nil {
		return dir
	}
	return abs
}
