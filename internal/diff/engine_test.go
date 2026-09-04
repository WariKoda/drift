package diff

import (
	"bytes"
	"errors"
	"io"
	"net/textproto"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/fs"
)

// testRoot returns a project root and its directory. Compare reads local files
// through fs.Root, so every test needs a real one.
func testRoot(t *testing.T) (*fs.Root, string) {
	t.Helper()
	dir := t.TempDir()
	root, err := fs.OpenRoot(dir)
	if err != nil {
		t.Fatalf("open project root: %v", err)
	}
	t.Cleanup(func() { _ = root.Close() })
	return root, dir
}

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

func (s stubRemoteClient) Open(path string) (io.ReadCloser, error) {
	if s.readErr != nil {
		return nil, s.readErr
	}
	return io.NopCloser(bytes.NewReader(s.readData)), nil
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
	root, dir := testRoot(t)
	remoteData := []byte("hello\nworld\n")
	result, err := Compare(root, filepath.Join(dir, "missing.txt"), "/remote/file.txt", stubRemoteClient{
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
	root, dir := testRoot(t)
	localPath := filepath.Join(dir, "local.txt")
	if err := os.WriteFile(localPath, []byte("hello\nworld\n"), 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	result, err := Compare(root, localPath, "/remote/missing.txt", stubRemoteClient{
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
	root, dir := testRoot(t)
	localPath := filepath.Join(dir, "local.txt")
	if err := os.WriteFile(localPath, []byte("hello\n"), 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	wantErr := errors.New("permission denied")
	result, err := Compare(root, localPath, "/remote/file.txt", stubRemoteClient{
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
	root, dir := testRoot(t)
	localPath := filepath.Join(dir, "local.txt")
	if err := os.WriteFile(localPath, []byte("hello\n"), 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	result, err := Compare(root, localPath, "/remote/missing.txt", stubRemoteClient{
		statErr: &textproto.Error{Code: 550, Msg: "File unavailable"},
	})
	if err != nil {
		t.Fatalf("Compare returned error: %v", err)
	}
	if !result.LocalOnly {
		t.Fatal("Compare did not treat FTP 550 as missing remote file")
	}
}

func TestCompare_BinaryFilesUseContentForEquality(t *testing.T) {
	tests := []struct {
		name       string
		remoteData []byte
		wantDiff   bool
	}{
		{name: "different", remoteData: []byte{0, 1, 2, 4}, wantDiff: true},
		{name: "identical", remoteData: []byte{0, 1, 2, 3}, wantDiff: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root, dir := testRoot(t)
			localData := []byte{0, 1, 2, 3}
			localPath := filepath.Join(dir, "local.bin")
			if err := os.WriteFile(localPath, localData, 0o644); err != nil {
				t.Fatalf("WriteFile failed: %v", err)
			}

			result, err := Compare(root, localPath, "/remote/file.bin", stubRemoteClient{
				statInfo: stubFileInfo{
					name:    "file.bin",
					size:    int64(len(tt.remoteData)),
					modTime: time.Now().Add(time.Minute),
				},
				readData: tt.remoteData,
			})
			if err != nil {
				t.Fatalf("Compare returned error: %v", err)
			}
			if !result.Binary {
				t.Fatal("Compare did not mark binary content as binary")
			}
			if got := result.HasDiff(); got != tt.wantDiff {
				t.Fatalf("HasDiff() = %v, want %v", got, tt.wantDiff)
			}
			if result.ContentDiff != tt.wantDiff {
				t.Fatalf("ContentDiff = %v, want %v", result.ContentDiff, tt.wantDiff)
			}
		})
	}
}

func TestCompare_LargeFilesUseStreamingContentComparison(t *testing.T) {
	tests := []struct {
		name     string
		mutate   bool
		wantDiff bool
	}{
		{name: "different", mutate: true, wantDiff: true},
		{name: "identical", wantDiff: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			localData := bytes.Repeat([]byte("a"), maxTextSize+1)
			remoteData := bytes.Clone(localData)
			if tt.mutate {
				remoteData[len(remoteData)-1] = 'b'
			}

			root, dir := testRoot(t)
			localPath := filepath.Join(dir, "large.dat")
			if err := os.WriteFile(localPath, localData, 0o644); err != nil {
				t.Fatalf("WriteFile failed: %v", err)
			}

			result, err := Compare(root, localPath, "/remote/large.dat", stubRemoteClient{
				statInfo: stubFileInfo{
					name:    "large.dat",
					size:    int64(len(remoteData)),
					modTime: time.Now().Add(time.Minute),
				},
				readData: remoteData,
			})
			if err != nil {
				t.Fatalf("Compare returned error: %v", err)
			}
			if got := result.HasDiff(); got != tt.wantDiff {
				t.Fatalf("HasDiff() = %v, want %v", got, tt.wantDiff)
			}
			if result.ContentDiff != tt.wantDiff {
				t.Fatalf("ContentDiff = %v, want %v", result.ContentDiff, tt.wantDiff)
			}
		})
	}
}

func TestCompare_LargeFilesWithDifferentSizesAlwaysDiffer(t *testing.T) {
	root, dir := testRoot(t)
	localData := bytes.Repeat([]byte("a"), maxTextSize+1)
	localPath := filepath.Join(dir, "large.dat")
	if err := os.WriteFile(localPath, localData, 0o644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}

	result, err := Compare(root, localPath, "/remote/large.dat", stubRemoteClient{
		statInfo: stubFileInfo{
			name:    "large.dat",
			size:    int64(len(localData) + 1),
			modTime: time.Now().Add(time.Minute),
		},
		readErr: errors.New("content should not be read when sizes differ"),
	})
	if err != nil {
		t.Fatalf("Compare returned error: %v", err)
	}
	if !result.HasDiff() || !result.ContentDiff {
		t.Fatal("Compare did not mark different large-file sizes as changed")
	}
}

func TestSplitLinesNormalisesCRLF(t *testing.T) {
	got := splitLines("a\r\nb\r\n}\r\n")
	want := []string{"a", "b", "}"}
	if len(got) != len(want) {
		t.Fatalf("splitLines returned %q, want %q", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("splitLines[%d] = %q, want %q", i, got[i], want[i])
		}
	}
}

func TestLineDiffDropsCarriageReturns(t *testing.T) {
	lines := lineDiff("{\r\n  \"a\": 1\r\n}\r\n", "{\r\n  \"a\": 2\r\n}\r\n")
	for _, line := range lines {
		if strings.Contains(line.Text, "\r") {
			t.Fatalf("diff line still contains CR: %q", line.Text)
		}
	}
}
