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
