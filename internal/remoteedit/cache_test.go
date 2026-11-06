package remoteedit

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestCleanupStaysBoundToOpenedCacheParent(t *testing.T) {
	s, conn, _ := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	parent := filepath.Dir(s.dir)
	moved := parent + "-moved"
	if err := os.Rename(parent, moved); err != nil {
		t.Fatal(err)
	}
	projectSession := filepath.Join(s.request.ProjectRoot, filepath.Base(s.dir))
	if err := os.Mkdir(projectSession, 0700); err != nil {
		t.Fatal(err)
	}
	sentinel := filepath.Join(projectSession, "do-not-remove")
	if err := os.WriteFile(sentinel, []byte("project"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(s.request.ProjectRoot, parent); err != nil {
		t.Fatal(err)
	}
	if err := s.Close(true); err != nil {
		t.Fatal(err)
	}
	if data, err := os.ReadFile(sentinel); err != nil || string(data) != "project" {
		t.Fatalf("cleanup followed a replaced cache parent into the project: %q, %v", data, err)
	}
	if _, err := os.Stat(filepath.Join(moved, filepath.Base(s.dir))); !os.IsNotExist(err) {
		t.Fatalf("cleanup did not remove the original session: %v", err)
	}
}
