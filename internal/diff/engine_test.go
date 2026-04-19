package diff

import (
	"errors"
	"net/textproto"
	"os"
	"path/filepath"
	"testing"
	"time"
)

type stubRemoteClient struct {
	statInfo os.FileInfo
	statErr  error
	readData []byte
	readErr  error
}

func (s stubRemoteClient) Stat(path string) (os.FileInfo, error) {
	return s.statInfo, s.statErr
}

func (s stubRemoteClient) ReadFile(path string) ([]byte, error) {
	return s.readData, s.readErr
}

type stubFileInfo struct {
	name    string
	size    int64
	modTime time.Time
	isDir   bool
}

func (s stubFileInfo) Name() string       { return s.name }
func (s stubFileInfo) Size() int64        { return s.size }
func (s stubFileInfo) Mode() os.FileMode  { return 0o644 }
func (s stubFileInfo) ModTime() time.Time { return s.modTime }
func (s stubFileInfo) IsDir() bool        { return s.isDir }
func (s stubFileInfo) Sys() any           { return nil }

func TestCompare_RemoteOnlyWhenLocalIsMissing(t *testing.T) {
	remoteData := []byte("hello\nworld\n")
	result, err := Compare("/definitely/missing/local.txt", "/remote/file.txt", stubRemoteClient{
		statInfo: stubFileInfo{name: "file.txt", size: int64(len(remoteData)), modTime: time.Now()},
		readData: remoteData,
	})
	if err != nil {
		t.Fatalf("Compare returned error: %v", err)
	}
	if !result.RemoteOnly {
		t.Fatal("Compare did not mark missing local file as remote-only")
	}
	if result.LocalOnly {
		t.Fatal("Compare incorrectly marked result as local-only")
	}
}

func TestCompare_LocalOnlyWhenRemoteIsMissing(t *testing.T) {
	dir := t.TempDir()
	localPath := filepath.Join(dir, "local.txt")
	if err := os.WriteFile(localPath, []byte("hello\nworld\n"), 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	result, err := Compare(localPath, "/remote/missing.txt", stubRemoteClient{
		statErr: os.ErrNotExist,
	})
	if err != nil {
		t.Fatalf("Compare returned error: %v", err)
	}
	if !result.LocalOnly {
		t.Fatal("Compare did not mark missing remote file as local-only")
	}
	if result.RemoteOnly {
		t.Fatal("Compare incorrectly marked result as remote-only")
	}
}

func TestCompare_ReturnsRemoteStatErrorsThatAreNotNotFound(t *testing.T) {
	dir := t.TempDir()
	localPath := filepath.Join(dir, "local.txt")
	if err := os.WriteFile(localPath, []byte("hello\n"), 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	wantErr := errors.New("permission denied")
	result, err := Compare(localPath, "/remote/file.txt", stubRemoteClient{
		statErr: wantErr,
	})
	if !errors.Is(err, wantErr) {
		t.Fatalf("Compare error = %v, want %v", err, wantErr)
	}
	if result.LocalOnly || result.RemoteOnly {
		t.Fatal("Compare incorrectly converted remote stat error into presence result")
	}
}

func TestCompare_TreatsFTP550AsNotFound(t *testing.T) {
	dir := t.TempDir()
	localPath := filepath.Join(dir, "local.txt")
	if err := os.WriteFile(localPath, []byte("hello\n"), 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	result, err := Compare(localPath, "/remote/missing.txt", stubRemoteClient{
		statErr: &textproto.Error{Code: 550, Msg: "File unavailable"},
	})
	if err != nil {
		t.Fatalf("Compare returned error: %v", err)
	}
	if !result.LocalOnly {
		t.Fatal("Compare did not treat FTP 550 as missing remote file")
	}
}
