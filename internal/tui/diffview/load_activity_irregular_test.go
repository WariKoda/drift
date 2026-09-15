//go:build unix

package diffview

import (
	"context"
	"os"
	"path/filepath"
	"syscall"
	"testing"
	"time"
)

func TestLoadActivityLocalWalkSkipsFifos(t *testing.T) {
	dir := t.TempDir()
	if err := syscall.Mkfifo(filepath.Join(dir, "pipe"), 0o600); err != nil {
		t.Skipf("mkfifo is unavailable here: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "real.txt"), []byte("content"), 0o600); err != nil {
		t.Fatal(err)
	}

	activity := newLoadActivity(context.Background(), time.Minute)
	defer activity.finish(false)
	var seen []string
	if err := activity.walkLocal(dir, func(path string) error {
		seen = append(seen, filepath.Base(path))
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if len(seen) != 1 || seen[0] != "real.txt" {
		t.Fatalf("walk visited %v, want only real.txt", seen)
	}
}
