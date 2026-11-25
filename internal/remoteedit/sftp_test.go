package remoteedit

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/sshtest"
	"github.com/WariKoda/drift/internal/tlstrust"
)

func TestSFTPEditingUsesRealSSHAndPreservesRemoteMode(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	remoteDir := t.TempDir()
	server := sshtest.Start(t, remoteDir)
	host := server.Host()
	// The user approved this host key for the session.
	trust := tlstrust.NewManager()
	trust.HostKeys().Trust(server.Challenge())
	conn, err := remote.Connect(context.Background(), host, trust, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	target := filepath.Join(remoteDir, "file.txt")
	if err := os.WriteFile(target, []byte("old"), 0750); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(target, 0750); err != nil {
		t.Fatal(err)
	}
	s := New(Request{Host: host, RemotePath: target, ProjectRoot: t.TempDir()})
	t.Cleanup(func() { _ = s.Close(true) })
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("new"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	if result := s.Save(context.Background(), conn, nil); result.Err != nil || !result.Saved {
		t.Fatalf("save: %+v", result)
	}
	info, err := os.Stat(target)
	if err != nil || info.Mode().Perm() != 0750 {
		t.Fatalf("remote mode: %v, %v", info, err)
	}
	link := filepath.Join(remoteDir, "link.txt")
	if err := os.Symlink(target, link); err != nil {
		t.Fatal(err)
	}
	linked := New(Request{Host: host, RemotePath: link, ProjectRoot: s.request.ProjectRoot})
	t.Cleanup(func() { _ = linked.Close(true) })
	if result := linked.Prepare(context.Background(), conn); !errors.Is(result.Err, ErrConflict) {
		t.Fatalf("remote symlink accepted: %+v", result)
	}
}
