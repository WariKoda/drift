package fs

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestClassifierSeparatesHiddenIgnoredAndHardExcluded(t *testing.T) {
	root := t.TempDir()
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "*.log\ncache/\n!keep.log\n")
	writeClassifierFile(t, filepath.Join(root, "nested", ".gitignore"), "secret.txt\n")

	classifier, err := NewClassifier(root)
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		path     string
		dir      bool
		hidden   bool
		ignored  bool
		excluded bool
	}{
		{".htaccess", false, true, false, false},
		{"error.log", false, false, true, false},
		{"keep.log", false, false, false, false},
		{"nested/secret.txt", false, false, true, false},
		{"cache", true, false, true, false},
		{".git/config", false, true, false, true},
		{"node_modules/a.js", false, false, false, true},
		{"node_modules", false, false, false, false},
	}
	for _, tc := range cases {
		got := classifyTest(t, classifier, filepath.Join(root, filepath.FromSlash(tc.path)), tc.dir)
		if got.Hidden != tc.hidden || got.Ignored != tc.ignored || got.HardExcluded != tc.excluded {
			t.Errorf("Classify(%q) = %+v", tc.path, got)
		}
	}
}

func TestClassifierKeepsTrackedIgnoredFile(t *testing.T) {
	root := t.TempDir()
	runGit(t, root, "init", "--quiet")
	writeClassifierFile(t, filepath.Join(root, "tracked.env"), "value\n")
	writeClassifierFile(t, filepath.Join(root, "ignored", "tracked.txt"), "value\n")
	runGit(t, root, "add", "tracked.env", "ignored/tracked.txt")
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "*.env\nignored/\n")

	classifier, err := NewClassifier(root)
	if err != nil {
		t.Fatal(err)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, "tracked.env"), false); got.Ignored {
		t.Fatalf("tracked file classified as ignored: %+v", got)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, "untracked.env"), false); !got.Ignored {
		t.Fatalf("untracked file not ignored: %+v", got)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, "ignored"), true); got.Ignored {
		t.Fatalf("directory containing tracked file classified as ignored: %+v", got)
	}
}

func TestClassifierUsesGlobalAndInfoExcludes(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	globalIgnore := filepath.Join(home, "global-ignore")
	writeClassifierFile(t, globalIgnore, "global.tmp\n")
	writeClassifierFile(t, filepath.Join(home, ".gitconfig"), "[core]\n\texcludesfile = "+globalIgnore+"\n")

	root := t.TempDir()
	runGit(t, root, "init", "--quiet")
	writeClassifierFile(t, filepath.Join(root, ".git", "info", "exclude"), "private.tmp\n")
	classifier, err := NewClassifier(root)
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"global.tmp", "private.tmp"} {
		if got := classifyTest(t, classifier, filepath.Join(root, name), false); !got.Ignored {
			t.Errorf("%s not ignored: %+v", name, got)
		}
	}
}

func TestClassifierMatchesGitForNestedAndNonexistentPaths(t *testing.T) {
	root := t.TempDir()
	runGit(t, root, "init", "--quiet")
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "build/\n!build/keep.txt\nfoo/**\n")
	writeClassifierFile(t, filepath.Join(root, "nested", ".gitignore"), "*.secret\n")
	classifier, err := NewClassifier(root)
	if err != nil {
		t.Fatal(err)
	}
	candidates := []ClassifyCandidate{
		{Path: filepath.Join(root, "build", "keep.txt")},
		{Path: filepath.Join(root, "nested", "remote.secret")},
		{Path: filepath.Join(root, "foo"), IsDir: true},
		{Path: filepath.Join(root, "foo", "remote.txt")},
	}
	classes, err := classifier.ClassifyBatch(t.Context(), candidates)
	if err != nil {
		t.Fatal(err)
	}
	wantIgnored := []bool{true, true, true, true}
	for i, want := range wantIgnored {
		if classes[i].Ignored != want {
			t.Errorf("candidate %q ignored = %t, want %t (%+v)", candidates[i].Path, classes[i].Ignored, want, classes[i])
		}
	}
}

func TestClassifierSupportsLinkedWorktreeIndexAndCommonExclude(t *testing.T) {
	mainRoot := t.TempDir()
	runGit(t, mainRoot, "init", "--quiet")
	runGit(t, mainRoot, "config", "user.email", "drift@example.test")
	runGit(t, mainRoot, "config", "user.name", "drift test")
	writeClassifierFile(t, filepath.Join(mainRoot, "tracked.txt"), "tracked")
	runGit(t, mainRoot, "add", "tracked.txt")
	runGit(t, mainRoot, "commit", "--quiet", "-m", "initial")
	writeClassifierFile(t, filepath.Join(mainRoot, ".git", "info", "exclude"), "remote.tmp\n")

	worktree := filepath.Join(t.TempDir(), "worktree")
	runGit(t, mainRoot, "worktree", "add", "--quiet", worktree)
	writeClassifierFile(t, filepath.Join(worktree, ".gitignore"), "*.txt\n")
	if err := os.Remove(filepath.Join(worktree, "tracked.txt")); err != nil {
		t.Fatal(err)
	}
	classifier, err := NewClassifier(worktree)
	if err != nil {
		t.Fatal(err)
	}
	if got := classifyTest(t, classifier, filepath.Join(worktree, "tracked.txt"), false); got.Ignored {
		t.Fatalf("missing tracked worktree file classified as ignored: %+v", got)
	}
	if got := classifyTest(t, classifier, filepath.Join(worktree, "remote.tmp"), false); !got.Ignored {
		t.Fatalf("common info/exclude was not applied: %+v", got)
	}
	if got := classifyTest(t, classifier, filepath.Join(worktree, ".git"), false); !got.HardExcluded {
		t.Fatalf("linked-worktree .git file was not fixed-excluded: %+v", got)
	}
}

func TestClassifierSupportsNewlinesAndMissingGitFailsClosed(t *testing.T) {
	root := t.TempDir()
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "*.tmp\n")
	name := "line\nbreak.tmp"
	writeClassifierFile(t, filepath.Join(root, name), "value")
	classifier, err := NewClassifier(root)
	if err != nil {
		t.Fatal(err)
	}
	if got := classifyTest(t, classifier, filepath.Join(root, name), false); !got.Ignored {
		t.Fatalf("newline path not ignored: %+v", got)
	}

	t.Setenv("PATH", t.TempDir())
	if _, err := NewClassifier(root); err == nil {
		t.Fatal("missing Git did not fail classifier initialization")
	}
}

func TestClassifierRejectsPathsOutsideProject(t *testing.T) {
	root := t.TempDir()
	classifier, err := NewClassifier(root)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := classifier.ClassifyBatch(t.Context(), []ClassifyCandidate{{Path: filepath.Join(root, "..", "outside")}}); err == nil {
		t.Fatal("outside path was accepted")
	}
}

func classifyTest(t *testing.T, classifier *Classifier, path string, isDir bool) PathClass {
	t.Helper()
	classes, err := classifier.ClassifyBatch(t.Context(), []ClassifyCandidate{{Path: path, IsDir: isDir}})
	if err != nil {
		t.Fatal(err)
	}
	return classes[0]
}

func runGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v: %s", args, err, output)
	}
}

func writeClassifierFile(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}
