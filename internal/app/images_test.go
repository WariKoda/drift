package app

import (
	"bytes"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"golang.org/x/image/bmp"
)

func testPicture(width, height int, shade uint8) *image.NRGBA {
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for y := range height {
		for x := range width {
			img.SetNRGBA(x, y, color.NRGBA{R: uint8(x), G: uint8(y), B: shade, A: 255})
		}
	}
	return img
}

func encodedPNG(t *testing.T, img image.Image) string {
	t.Helper()
	var buffer bytes.Buffer
	if err := png.Encode(&buffer, img); err != nil {
		t.Fatal(err)
	}
	return buffer.String()
}

func imageTestEnv(t *testing.T, local string) (*fs.Root, *ftptest.Server, string) {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "logo.png")
	if local != "" {
		if err := os.WriteFile(path, []byte(local), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	root, err := fs.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { root.Close() })
	return root, ftptest.Start(t, 1), path
}

func TestLoadImagePairReadsBothSidesOverFTP(t *testing.T) {
	root, server, localPath := imageTestEnv(t, encodedPNG(t, testPicture(300, 100, 10)))
	server.AddFile("/logo.png", encodedPNG(t, testPicture(120, 60, 200)))
	conn := connectTestHost(t, server.Host(t))

	pair := LoadImagePair(root, conn, localPath, "/logo.png", false, false, 150)
	if pair.Local.Err != nil || pair.Remote.Err != nil {
		t.Fatalf("errors: local %v, remote %v", pair.Local.Err, pair.Remote.Err)
	}
	if pair.Local.Info.Width != 300 || pair.Remote.Info.Width != 120 {
		t.Fatalf("info: local %+v, remote %+v", pair.Local.Info, pair.Remote.Info)
	}
	if got := pair.Local.Image.Bounds().Size(); got != image.Pt(150, 50) {
		t.Fatalf("local display size = %v", got)
	}
	if got := pair.Remote.Image.Bounds().Size(); got != image.Pt(120, 60) {
		t.Fatalf("remote display size = %v", got)
	}
	if pair.SamePixels {
		t.Fatal("different images reported as equal")
	}
}

func TestLoadImagePairDetectsReencodedImages(t *testing.T) {
	picture := testPicture(64, 64, 90)
	root, server, localPath := imageTestEnv(t, encodedPNG(t, picture))
	var remote bytes.Buffer
	if err := bmp.Encode(&remote, picture); err != nil {
		t.Fatal(err)
	}
	server.AddFile("/logo.png", remote.String())
	conn := connectTestHost(t, server.Host(t))

	pair := LoadImagePair(root, conn, localPath, "/logo.png", false, false, 32)
	if !pair.SamePixels {
		t.Fatal("the same pixels in PNG and BMP were not recognized")
	}
	if pair.Local.Info.Format != "png" || pair.Remote.Info.Format != "bmp" {
		t.Fatalf("formats: %s, %s", pair.Local.Info.Format, pair.Remote.Info.Format)
	}
}

func TestLoadImagePairReportsEachSide(t *testing.T) {
	root, server, localPath := imageTestEnv(t, "")
	server.AddFile("/logo.png", "not an image")
	conn := connectTestHost(t, server.Host(t))

	pair := LoadImagePair(root, conn, localPath, "/logo.png", true, false, 32)
	if !pair.Local.Missing || pair.Local.Err != nil {
		t.Fatalf("local side = %+v", pair.Local)
	}
	if pair.Remote.Err == nil || !strings.Contains(pair.Remote.Err.Error(), "not a supported image") || pair.Remote.Image != nil {
		t.Fatalf("remote side = %+v", pair.Remote)
	}
	if pair.SamePixels {
		t.Fatal("a missing side cannot have the same pixels")
	}
}
