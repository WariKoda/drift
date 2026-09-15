package sftp

import (
	"os"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

// The remote walker feeds the comparison, which stats and opens every path it
// gets. A symlink handed over here would be followed and its target downloaded
// as an ordinary file, while the local walker skips symlinks.
func TestWalkFilesSkipsRemoteSymlinks(t *testing.T) {
	server := newLocalSSHServer(t, false)
	if err := os.WriteFile(filepath.Join(server.root, "real.txt"), []byte("content"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(server.root, "target.txt"), []byte("target"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(server.root, "target.txt"), filepath.Join(server.root, "link.txt")); err != nil {
		t.Fatal(err)
	}

	client := connectLocal(t, server, 0, time.Second)
	var got []string
	if err := client.WalkFiles(server.root, func(p string) error {
		got = append(got, filepath.Base(p))
		return nil
	}); err != nil {
		t.Fatal(err)
	}

	sort.Strings(got)
	want := []string{"real.txt", "target.txt"}
	if len(got) != len(want) {
		t.Fatalf("walk returned %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("walk returned %v, want %v", got, want)
		}
	}
}
