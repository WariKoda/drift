package fs

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// A project directory can come from a remote host or an unpacked archive, so its
// Git configuration is untrusted. core.fsmonitor names a program Git starts when
// it loads the index, which check-ignore does in a repository.
func TestClassifierDoesNotRunRepositoryFsmonitor(t *testing.T) {
	root := t.TempDir()
	marker := filepath.Join(t.TempDir(), "executed")
	runGit(t, root, "init", "--quiet")
	runGit(t, root, "config", "core.fsmonitor", fsmonitorCommand(marker))
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "*.log\n")
	requireFsmonitorRuns(t, root, marker)

	classifier, err := NewClassifier(t.Context(), root)
	if err != nil {
		t.Fatal(err)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, "error.log"), false); !got.Ignored {
		t.Fatalf("ignore rule not applied: %+v", got)
	}
	requireNotExecuted(t, marker)
}

// Git also accepts a directory holding HEAD, objects/ and refs/ as a repository.
// None of those names are excluded from a download, so a remote host can turn a
// project without .git into one whose config Git would read.
func TestClassifierIgnoresBareRepositoryLayoutInProject(t *testing.T) {
	root := t.TempDir()
	marker := filepath.Join(t.TempDir(), "executed")
	writeClassifierFile(t, filepath.Join(root, "HEAD"), "ref: refs/heads/main\n")
	writeClassifierFile(t, filepath.Join(root, "objects", "x"), "")
	writeClassifierFile(t, filepath.Join(root, "refs", "x"), "")
	writeClassifierFile(t, filepath.Join(root, "config"),
		"[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tworktree = .\n\tfsmonitor = \""+
			fsmonitorCommand(marker)+"\"\n")
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "*.log\n")
	requireFsmonitorRuns(t, root, marker)

	classifier, err := NewClassifier(t.Context(), root)
	if err != nil {
		t.Fatal(err)
	}
	if classifier.repoRoot != "" {
		t.Fatalf("downloaded files were accepted as repository %q", classifier.repoRoot)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, "error.log"), false); !got.Ignored {
		t.Fatalf("ignore rule not applied: %+v", got)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, "config"), false); got.Ignored || got.HardExcluded {
		t.Fatalf("ordinary file name classified as excluded: %+v", got)
	}
	requireNotExecuted(t, marker)
}

func fsmonitorCommand(marker string) string {
	return "touch '" + marker + "'; false"
}

// requireFsmonitorRuns proves the fixture is live: an unprotected check-ignore
// in root starts the configured program. Without that, the absence of the
// marker after classification would show nothing.
func requireFsmonitorRuns(t *testing.T, root, marker string) {
	t.Helper()
	cmd := exec.Command("git", "-C", root, "check-ignore", "--quiet", "probe")
	if output, err := cmd.CombinedOutput(); err != nil {
		var exitErr *exec.ExitError
		if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
			t.Skipf("this Git does not accept the fixture: %v: %s", err, output)
		}
	}
	if _, err := os.Stat(marker); err != nil {
		t.Skipf("this Git does not start core.fsmonitor for check-ignore: %v", err)
	}
	if err := os.Remove(marker); err != nil {
		t.Fatal(err)
	}
}

func requireNotExecuted(t *testing.T, marker string) {
	t.Helper()
	if _, err := os.Stat(marker); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("Git ran the program configured in the project (stat error: %v)", err)
	}
}
