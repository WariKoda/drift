package fs

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestClassifierWithNonEnglishLocale(t *testing.T) {
	t.Setenv("LC_ALL", "en_US.UTF-8")
	t.Setenv("LANG", "en_US.UTF-8")
	t.Setenv("LANGUAGE", "de")
	t.Setenv("HOME", t.TempDir())
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	for _, repository := range []bool{false, true} {
		name := "outside repository"
		if repository {
			name = "inside repository"
		}
		t.Run(name, func(t *testing.T) {
			root := t.TempDir()
			if repository {
				runGit(t, root, "init", "--quiet")
			}
			writeClassifierFile(t, filepath.Join(root, ".gitignore"), "*.log\ngeheim-*.txt\n")
			writeClassifierFile(t, filepath.Join(root, "geheim-ä.txt"), "ignored")
			writeClassifierFile(t, filepath.Join(root, "sichtbar-ö.txt"), "visible")

			classifier, err := NewClassifier(t.Context(), root)
			if err != nil {
				t.Fatal(err)
			}
			for filename, ignored := range map[string]bool{
				"geheim-ä.txt":    true,
				"sichtbar-ö.txt":  false,
				"remote-only.log": true,
			} {
				class := classifyTest(t, classifier, filepath.Join(root, filename), false)
				if class.Ignored != ignored {
					t.Errorf("Classify(%q).Ignored = %t, want %t", filename, class.Ignored, ignored)
				}
			}
			if !repository {
				if _, err := os.Stat(filepath.Join(root, ".git")); !os.IsNotExist(err) {
					t.Fatalf("classification wrote repository metadata into the project: %v", err)
				}
			}
		})
	}

	t.Run("corrupt repository still fails", func(t *testing.T) {
		root := t.TempDir()
		runGit(t, root, "init", "--quiet")
		writeClassifierFile(t, filepath.Join(root, ".git", "config"), "[broken\n")
		_, err := NewClassifier(t.Context(), root)
		var exitErr *exec.ExitError
		if !errors.As(err, &exitErr) || exitErr.ExitCode() != 128 {
			t.Fatalf("corrupt repository error = %v, want Git exit 128", err)
		}
		if !strings.Contains(err.Error(), "find Git worktree") || isNotRepository(err) {
			t.Fatalf("corrupt repository was treated as a missing repository: %v", err)
		}
	})

	for key, want := range map[string]string{"LC_ALL": "en_US.UTF-8", "LANG": "en_US.UTF-8", "LANGUAGE": "de"} {
		if got := os.Getenv(key); got != want {
			t.Errorf("classifier changed the application environment: %s = %q, want %q", key, got, want)
		}
	}
}

func TestClassifierGitCommandPreservesEnvironment(t *testing.T) {
	t.Setenv("LC_ALL", "en_US.UTF-8")
	t.Setenv("LANG", "en_US.UTF-8")
	t.Setenv("LANGUAGE", "de")
	t.Setenv("HOME", t.TempDir())
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	t.Setenv("GIT_CONFIG_COUNT", "1")
	t.Setenv("GIT_CONFIG_KEY_0", "core.quotePath")
	t.Setenv("GIT_CONFIG_VALUE_0", "false")

	cmd := classifierGitCommand(t.Context(), "-C", t.TempDir(), "config", "--get", "core.quotePath")
	for key, want := range map[string]string{"LC_ALL": "C", "LANG": "en_US.UTF-8", "LANGUAGE": "de"} {
		if !slices.Contains(cmd.Environ(), key+"="+want) {
			t.Errorf("Git environment missing %s=%s", key, want)
		}
	}
	if slices.Contains(cmd.Environ(), "LC_ALL=en_US.UTF-8") {
		t.Error("Git environment retained the conflicting LC_ALL value")
	}
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("read inherited Git config: %v: %s", err, output)
	}
	if strings.TrimSpace(string(output)) != "false" {
		t.Fatalf("inherited Git config = %q, want false", output)
	}
	if got := os.Getenv("LC_ALL"); got != "en_US.UTF-8" {
		t.Fatalf("application locale changed to %q", got)
	}
}

func TestClassifierGitCommandHonorsCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	if _, err := classifierGitCommand(ctx, "--version").Output(); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled Git command returned %v", err)
	}
}
