package diff

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/fs"
)

func TestLocalReadActivityAndCancellation(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "file")
	content := strings.Repeat("content", 10000)
	if err := os.WriteFile(path, []byte(content), 0600); err != nil {
		t.Fatal(err)
	}
	root, err := fs.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	calls := 0
	data, err := readLocal(root, path, func() error { calls++; return nil })
	if err != nil || string(data) != content || calls < 3 {
		t.Fatalf("local read: bytes=%d calls=%d error=%v", len(data), calls, err)
	}
	stopped := errors.New("stop reading")
	if _, err := readLocal(root, path, func() error { return stopped }); !errors.Is(err, stopped) {
		t.Fatalf("local cancellation: %v", err)
	}
	file, err := root.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	calls = 0
	_, size, err := digestAndClose(&activityReader{ReadCloser: file, activity: func() error {
		calls++
		if calls >= 3 {
			return stopped
		}
		return nil
	}})
	if !errors.Is(err, stopped) || size == 0 || size >= int64(len(content)) {
		t.Fatalf("hash cancellation: size=%d err=%v", size, err)
	}
	if _, err := file.Stat(); !errors.Is(err, os.ErrClosed) {
		t.Fatalf("canceled hash left file open: %v", err)
	}
}
