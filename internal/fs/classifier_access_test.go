package fs

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestClassifierDoesNotWalkUnrequestedDirectories(t *testing.T) {
	root := t.TempDir()
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "var/cache/\n")
	file := filepath.Join(root, "src", "source.go")
	writeClassifierFile(t, file, "package source\n")
	cache := filepath.Join(root, "var", "cache")
	writeClassifierFile(t, filepath.Join(cache, "private.txt"), "cache\n")
	if err := os.Chmod(cache, 0); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.Chmod(cache, 0o700); err != nil {
			t.Error(err)
		}
	})
	if _, err := os.ReadDir(cache); err == nil {
		t.Skip("directory permissions do not prevent reads for this user")
	} else if !errors.Is(err, os.ErrPermission) {
		t.Fatal(err)
	}

	classifier, err := NewClassifier(t.Context(), root)
	if err != nil {
		t.Fatalf("initialize without walking the ignored cache: %v", err)
	}
	classes, err := classifier.ClassifyBatch(t.Context(), []ClassifyCandidate{{Path: file}, {Path: cache, IsDir: true}})
	if err != nil {
		t.Fatal(err)
	}
	if classes[0].Ignored || classes[0].HardExcluded || !classes[1].Ignored {
		t.Fatalf("source and cache classifications = %+v", classes)
	}
}

func TestClassifierCachesRequestedPathsUntilNewInstance(t *testing.T) {
	root := t.TempDir()
	file := filepath.Join(root, "file.txt")
	writeClassifierFile(t, file, "source\n")
	classifier, err := NewClassifier(t.Context(), root)
	if err != nil {
		t.Fatal(err)
	}
	if _, cached := classifier.CachedClassify(file, false); cached {
		t.Fatal("initialization classified an unrequested path")
	}
	if got := classifyTest(t, classifier, file, false); got.Ignored {
		t.Fatalf("file initially ignored: %+v", got)
	}
	writeClassifierFile(t, filepath.Join(root, ".gitignore"), "file.txt\n")
	if got := classifyTest(t, classifier, file, false); got.Ignored {
		t.Fatalf("cached classification changed within one instance: %+v", got)
	}
	refreshed, err := NewClassifier(t.Context(), root)
	if err != nil {
		t.Fatal(err)
	}
	if got := classifyTest(t, refreshed, file, false); !got.Ignored {
		t.Fatalf("new instance did not apply changed ignore rules: %+v", got)
	}
}
