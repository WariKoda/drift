package sftp

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestWalkActivityIncludesEmptyDirectoriesAndCanStop(t *testing.T) {
	server := newLocalSSHServer(t, false)
	for _, dir := range []string{"a/b", "c"} {
		if err := os.MkdirAll(filepath.Join(server.root, dir), 0700); err != nil {
			t.Fatal(err)
		}
	}
	client := connectLocal(t, server, 0, time.Second)
	visits := 0
	files := 0
	if err := client.WalkFilesWithActivity(server.root, func(string) error { files++; return nil }, func() error { visits++; return nil }); err != nil {
		t.Fatal(err)
	}
	if files != 0 || visits != 4 {
		t.Fatalf("visited %d entries, %d files; want root and three empty directories", visits, files)
	}
	stopped := errors.New("stop walk")
	if err := client.WalkFilesWithActivity(server.root, func(string) error { t.Fatal("file callback after stop"); return nil }, func() error { return stopped }); !errors.Is(err, stopped) {
		t.Fatalf("walk: %v", err)
	}
}
