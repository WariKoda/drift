package sftp

import (
	"errors"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"

	pkgsftp "github.com/pkg/sftp"

	"github.com/WariKoda/drift/internal/fs"
)

// failingReader yields a little content and then fails, so a transfer breaks
// after the staging file has already been created.
type failingReader struct{ read bool }

func (r *failingReader) Read(p []byte) (int, error) {
	if r.read {
		return 0, errors.New("source read failed")
	}
	r.read = true
	return copy(p, "partial"), nil
}

func (r *failingReader) Close() error { return nil }

func TestUploadReplacesTargetAtomically(t *testing.T) {
	client, remoteDir := newProtocolClient(t)
	remoteTarget := filepath.Join(remoteDir, "target.txt")
	if err := os.WriteFile(remoteTarget, []byte("old content"), 0o750); err != nil {
		t.Fatal(err)
	}

	if err := client.Upload("target.txt", strings.NewReader("new content")); err != nil {
		t.Fatalf("Upload returned error: %v", err)
	}

	assertFileContent(t, remoteTarget, "new content")
	info, err := os.Stat(remoteTarget)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := info.Mode().Perm(), os.FileMode(0o750); got != want {
		t.Fatalf("remote target mode = %o, want %o", got, want)
	}
	assertNoStagingFiles(t, remoteDir)
}

func TestUploadFailurePreservesTarget(t *testing.T) {
	client, remoteDir := newProtocolClient(t)
	remoteTarget := filepath.Join(remoteDir, "target.txt")
	if err := os.WriteFile(remoteTarget, []byte("old content"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := client.Upload("target.txt", &failingReader{}); err == nil {
		t.Fatal("Upload unexpectedly succeeded for a failing source")
	}

	assertFileContent(t, remoteTarget, "old content")
	assertNoStagingFiles(t, remoteDir)
}

func TestUploadRejectsSymlinkTarget(t *testing.T) {
	client, remoteDir := newProtocolClient(t)
	realTarget := filepath.Join(remoteDir, "real-target.txt")
	if err := os.WriteFile(realTarget, []byte("old content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("real-target.txt", filepath.Join(remoteDir, "target.txt")); err != nil {
		t.Fatal(err)
	}

	if err := client.Upload("target.txt", strings.NewReader("new content")); err == nil {
		t.Fatal("Upload unexpectedly replaced a symlink target")
	}

	assertFileContent(t, realTarget, "old content")
	linkTarget, err := os.Readlink(filepath.Join(remoteDir, "target.txt"))
	if err != nil {
		t.Fatal(err)
	}
	if linkTarget != "real-target.txt" {
		t.Fatalf("symlink target = %q, want %q", linkTarget, "real-target.txt")
	}
	assertNoStagingFiles(t, remoteDir)
}

// The download side is Open plus fs.Root.WriteAtomic. These tests keep that
// pair covered against the real SFTP protocol; fs.Root's own tests cover the
// project confinement.
func downloadThroughRoot(t *testing.T, client *Client, remotePath, localPath, projectRoot string) error {
	t.Helper()
	root, err := fs.OpenRoot(projectRoot)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()

	src, err := client.Open(remotePath)
	if err != nil {
		return err
	}
	return root.WriteAtomic(localPath, src)
}

func TestDownloadReplacesTargetAtomically(t *testing.T) {
	client, remoteDir := newProtocolClient(t)
	if err := os.WriteFile(filepath.Join(remoteDir, "source.txt"), []byte("remote content"), 0o644); err != nil {
		t.Fatal(err)
	}
	localDir := t.TempDir()
	localTarget := filepath.Join(localDir, "target.txt")
	if err := os.WriteFile(localTarget, []byte("old content"), 0o750); err != nil {
		t.Fatal(err)
	}

	if err := downloadThroughRoot(t, client, "source.txt", localTarget, localDir); err != nil {
		t.Fatalf("download returned error: %v", err)
	}

	assertFileContent(t, localTarget, "remote content")
	info, err := os.Stat(localTarget)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := info.Mode().Perm(), os.FileMode(0o750); got != want {
		t.Fatalf("local target mode = %o, want %o", got, want)
	}
	assertNoStagingFiles(t, localDir)
}

func TestDownloadRejectsSymlinkTarget(t *testing.T) {
	client, remoteDir := newProtocolClient(t)
	if err := os.WriteFile(filepath.Join(remoteDir, "source.txt"), []byte("remote content"), 0o644); err != nil {
		t.Fatal(err)
	}
	localDir := t.TempDir()
	realTarget := filepath.Join(localDir, "real-target.txt")
	if err := os.WriteFile(realTarget, []byte("old content"), 0o644); err != nil {
		t.Fatal(err)
	}
	localTarget := filepath.Join(localDir, "target.txt")
	if err := os.Symlink("real-target.txt", localTarget); err != nil {
		t.Fatal(err)
	}

	if err := downloadThroughRoot(t, client, "source.txt", localTarget, localDir); err == nil {
		t.Fatal("download unexpectedly replaced a symlink target")
	}

	assertFileContent(t, realTarget, "old content")
	linkTarget, err := os.Readlink(localTarget)
	if err != nil {
		t.Fatal(err)
	}
	if linkTarget != "real-target.txt" {
		t.Fatalf("symlink target = %q, want %q", linkTarget, "real-target.txt")
	}
	assertNoStagingFiles(t, localDir)
}

func TestDownloadFailurePreservesTarget(t *testing.T) {
	client, remoteDir := newProtocolClient(t)
	if err := os.Mkdir(filepath.Join(remoteDir, "source-dir"), 0o755); err != nil {
		t.Fatal(err)
	}
	localDir := t.TempDir()
	localTarget := filepath.Join(localDir, "target.txt")
	if err := os.WriteFile(localTarget, []byte("old content"), 0o644); err != nil {
		t.Fatal(err)
	}

	// The SFTP server opens the directory, then fails when the client tries to
	// read it after the local staging file has been created.
	if err := downloadThroughRoot(t, client, "source-dir", localTarget, localDir); err == nil {
		t.Fatal("download unexpectedly succeeded for a directory source")
	}

	assertFileContent(t, localTarget, "old content")
	assertNoStagingFiles(t, localDir)
}

func newProtocolClient(t *testing.T) (*Client, string) {
	t.Helper()

	remoteDir := t.TempDir()
	serverConn, clientConn := net.Pipe()
	server, err := pkgsftp.NewServer(
		serverConn,
		pkgsftp.WithServerWorkingDirectory(remoteDir),
	)
	if err != nil {
		serverConn.Close()
		clientConn.Close()
		t.Fatal(err)
	}

	serveDone := make(chan error, 1)
	go func() {
		serveDone <- server.Serve()
	}()

	protocolClient, err := pkgsftp.NewClientPipe(clientConn, clientConn)
	if err != nil {
		serverConn.Close()
		clientConn.Close()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = protocolClient.Close()
		_ = clientConn.Close()
		_ = serverConn.Close()
		<-serveDone
	})

	return &Client{sftp: protocolClient}, remoteDir
}

func assertFileContent(t *testing.T, path, want string) {
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
