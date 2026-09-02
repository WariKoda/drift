package fs

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func openTestRoot(t *testing.T, dir string) *Root {
	t.Helper()
	root, err := OpenRoot(dir)
	if err != nil {
		t.Fatalf("OpenRoot(%s): %v", dir, err)
	}
	t.Cleanup(func() { _ = root.Close() })
	return root
}

// escapeProject builds a project whose "output" directory is a symlink to a
// directory outside the project, and puts a file called secret.txt in there.
// This is the shape from the audit: project/output/secret.txt passes every
// lexical check and still lands outside the project.
func escapeProject(t *testing.T) (root *Root, project, outside string) {
	t.Helper()
	base := t.TempDir()
	project = filepath.Join(base, "project")
	outside = filepath.Join(base, "outside")
	for _, dir := range []string{project, outside} {
		if err := os.Mkdir(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(outside, "secret.txt"), []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(project, "output")); err != nil {
		t.Fatal(err)
	}
	return openTestRoot(t, project), project, outside
}

func TestRootRefusesWriteThroughSymlinkedDirectory(t *testing.T) {
	root, project, outside := escapeProject(t)
	target := filepath.Join(project, "output", "secret.txt")

	err := root.WriteAtomic(target, io.NopCloser(strings.NewReader("overwritten")))
	if err == nil {
		t.Fatal("WriteAtomic wrote through a symlinked directory")
	}

	assertContent(t, filepath.Join(outside, "secret.txt"), "secret")
	assertNoStagingFiles(t, outside)
}

func TestRootRefusesDeleteThroughSymlinkedDirectory(t *testing.T) {
	root, project, outside := escapeProject(t)

	if err := root.Remove(filepath.Join(project, "output", "secret.txt")); err == nil {
		t.Fatal("Remove deleted through a symlinked directory")
	}
	assertContent(t, filepath.Join(outside, "secret.txt"), "secret")
}

func TestRootRefusesReadThroughSymlinkedDirectory(t *testing.T) {
	root, project, _ := escapeProject(t)
	target := filepath.Join(project, "output", "secret.txt")

	if _, err := root.Open(target); err == nil {
		t.Error("Open read through a symlinked directory")
	}
	if _, err := root.ReadFile(target); err == nil {
		t.Error("ReadFile read through a symlinked directory")
	}
	if _, err := root.Stat(target); err == nil {
		t.Error("Stat resolved through a symlinked directory")
	}
}

func TestRootRefusesPathsOutsideProject(t *testing.T) {
	base := t.TempDir()
	project := filepath.Join(base, "project")
	if err := os.Mkdir(project, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(base, "sibling.txt"), []byte("sibling"), 0o644); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	for _, path := range []string{
		filepath.Join(base, "sibling.txt"),
		filepath.Join(project, "..", "sibling.txt"),
		"relative.txt",
	} {
		if _, err := root.Open(path); err == nil {
			t.Errorf("Open(%s) succeeded, want refusal", path)
		}
		if err := root.Remove(path); err == nil {
			t.Errorf("Remove(%s) succeeded, want refusal", path)
		}
		if err := root.WriteAtomic(path, io.NopCloser(strings.NewReader("x"))); err == nil {
			t.Errorf("WriteAtomic(%s) succeeded, want refusal", path)
		}
	}
	assertContent(t, filepath.Join(base, "sibling.txt"), "sibling")
}

// Symlinks that stay inside the project are ordinary files and must keep
// working: the point is confinement, not banning symlinks.
func TestRootFollowsSymlinksInsideProject(t *testing.T) {
	project := t.TempDir()
	if err := os.Mkdir(filepath.Join(project, "real"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(project, "real", "file.txt"), []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("real", filepath.Join(project, "link")); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	data, err := root.ReadFile(filepath.Join(project, "link", "file.txt"))
	if err != nil {
		t.Fatalf("ReadFile through an internal symlink: %v", err)
	}
	if string(data) != "content" {
		t.Fatalf("content = %q, want %q", data, "content")
	}
}

func TestWriteAtomicReplacesTargetAndKeepsMode(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "sub", "target.txt")
	if err := os.Mkdir(filepath.Join(project, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("old"), 0o750); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	if err := root.WriteAtomic(target, io.NopCloser(strings.NewReader("new"))); err != nil {
		t.Fatalf("WriteAtomic: %v", err)
	}

	assertContent(t, target, "new")
	info, err := os.Stat(target)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := info.Mode().Perm(), os.FileMode(0o750); got != want {
		t.Fatalf("target mode = %o, want %o", got, want)
	}
	assertNoStagingFiles(t, filepath.Join(project, "sub"))
}

func TestWriteAtomicCreatesMissingParents(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "deep", "deeper", "new.txt")
	root := openTestRoot(t, project)

	if err := root.WriteAtomic(target, io.NopCloser(strings.NewReader("content"))); err != nil {
		t.Fatalf("WriteAtomic: %v", err)
	}
	assertContent(t, target, "content")
}

func TestWriteAtomicRefusesSymlinkTarget(t *testing.T) {
	project := t.TempDir()
	real := filepath.Join(project, "real.txt")
	if err := os.WriteFile(real, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(project, "link.txt")
	if err := os.Symlink("real.txt", target); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	if err := root.WriteAtomic(target, io.NopCloser(strings.NewReader("new"))); err == nil {
		t.Fatal("WriteAtomic replaced a symlink target")
	}
	assertContent(t, real, "old")
	if _, err := os.Readlink(target); err != nil {
		t.Fatalf("symlink was replaced: %v", err)
	}
	assertNoStagingFiles(t, project)
}

func TestWriteAtomicKeepsTargetWhenSourceFails(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "target.txt")
	if err := os.WriteFile(target, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	if err := root.WriteAtomic(target, &failingSource{}); err == nil {
		t.Fatal("WriteAtomic succeeded for a failing source")
	}
	assertContent(t, target, "old")
	assertNoStagingFiles(t, project)
}

// A download that only reports its failure on Close must not reach the target:
// for FTP, closing the stream is what confirms the transfer completed.
func TestWriteAtomicKeepsTargetWhenCloseFails(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "target.txt")
	if err := os.WriteFile(target, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	src := &closeFailingSource{Reader: strings.NewReader("truncated payload")}
	if err := root.WriteAtomic(target, src); err == nil {
		t.Fatal("WriteAtomic succeeded although the source failed on close")
	}
	assertContent(t, target, "old")
	assertNoStagingFiles(t, project)
}

func TestWriteAtomicClosesSourceOnRefusedPath(t *testing.T) {
	root, project, _ := escapeProject(t)
	src := &closeFailingSource{Reader: strings.NewReader("payload")}

	if err := root.WriteAtomic(filepath.Join(project, "output", "secret.txt"), src); err == nil {
		t.Fatal("WriteAtomic wrote through a symlinked directory")
	}
	if !src.closed {
		t.Error("WriteAtomic leaked the source stream when it refused the path")
	}
}

func TestRemoveDeletesInsideProject(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "gone.txt")
	if err := os.WriteFile(target, []byte("bye"), 0o644); err != nil {
		t.Fatal(err)
	}
	root := openTestRoot(t, project)

	if err := root.Remove(target); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if _, err := os.Lstat(target); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("file still present, Lstat error = %v", err)
	}
}

func TestCloseIsIdempotent(t *testing.T) {
	root, err := OpenRoot(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if err := root.Close(); err != nil {
		t.Fatalf("first Close: %v", err)
	}
	if err := root.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}
}

// failingSource yields a little content and then fails, so the write breaks
// after the staging file exists.
type failingSource struct{ read bool }

func (s *failingSource) Read(p []byte) (int, error) {
	if s.read {
		return 0, errors.New("source read failed")
	}
	s.read = true
	return copy(p, "partial"), nil
}

func (s *failingSource) Close() error { return nil }

// closeFailingSource reads fine and fails on Close, like a remote stream whose
// transfer turns out to be incomplete.
type closeFailingSource struct {
	*strings.Reader
	closed bool
}

func (s *closeFailingSource) Close() error {
	s.closed = true
	return errors.New("incomplete transfer")
}

func assertContent(t *testing.T, path, want string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if got := string(data); got != want {
		t.Fatalf("%s content = %q, want %q", path, got, want)
	}
}

func assertNoStagingFiles(t *testing.T, dir string) {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range entries {
		if strings.Contains(entry.Name(), ".drift-tmp-") {
			t.Fatalf("staging file was not cleaned up: %s", filepath.Join(dir, entry.Name()))
		}
	}
}
