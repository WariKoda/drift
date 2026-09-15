//go:build unix

package fs

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"
	"time"
)

// mkfifo creates a named pipe with no writer. Opening it for reading blocks
// until one appears, which is exactly the hang the guards are there to prevent.
func mkfifo(t *testing.T, path string) {
	t.Helper()
	if err := syscall.Mkfifo(path, 0o600); err != nil {
		t.Skipf("mkfifo is unavailable here: %v", err)
	}
}

func TestWalkFilesSkipsFifos(t *testing.T) {
	dir := t.TempDir()
	mkfifo(t, filepath.Join(dir, "pipe"))
	if err := os.WriteFile(filepath.Join(dir, "real.txt"), []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}

	var seen []string
	if err := WalkFiles(dir, func(path string) error {
		seen = append(seen, filepath.Base(path))
		return nil
	}); err != nil {
		t.Fatalf("WalkFiles returned error: %v", err)
	}

	if len(seen) != 1 || seen[0] != "real.txt" {
		t.Fatalf("WalkFiles visited %v, want only real.txt", seen)
	}
}

func TestReadDirSkipsFifos(t *testing.T) {
	dir := t.TempDir()
	mkfifo(t, filepath.Join(dir, "pipe"))
	if err := os.WriteFile(filepath.Join(dir, "real.txt"), []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}

	entries, err := ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 1 || entries[0].Name != "real.txt" {
		t.Fatalf("ReadDir returned %v, want only real.txt", entries)
	}
}

func TestRootOpenRefusesFifo(t *testing.T) {
	dir := t.TempDir()
	pipe := filepath.Join(dir, "pipe")
	mkfifo(t, pipe)

	root, err := OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()

	done := make(chan error, 1)
	go func() {
		f, err := root.Open(pipe)
		if err == nil {
			f.Close()
		}
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("Root.Open accepted a FIFO")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Root.Open blocked on a FIFO")
	}
}

func TestRootReadFileRefusesFifo(t *testing.T) {
	dir := t.TempDir()
	pipe := filepath.Join(dir, "pipe")
	mkfifo(t, pipe)

	root, err := OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()

	done := make(chan error, 1)
	go func() {
		_, err := root.ReadFile(pipe)
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("Root.ReadFile accepted a FIFO")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Root.ReadFile blocked on a FIFO")
	}
}
