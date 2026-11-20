package imagefile

import (
	"bytes"
	"encoding/binary"
	"errors"
	"hash/crc32"
	"image"
	"image/color"
	"image/png"
	"testing"
)

func encodePNG(t *testing.T, width, height int) []byte {
	t.Helper()
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for y := range height {
		for x := range width {
			img.Set(x, y, color.NRGBA{R: uint8(x), G: uint8(y), B: 200, A: 255})
		}
	}
	var buffer bytes.Buffer
	if err := png.Encode(&buffer, img); err != nil {
		t.Fatal(err)
	}
	return buffer.Bytes()
}

func TestProbeReadsHeader(t *testing.T) {
	info, ok := Probe(encodePNG(t, 30, 20))
	if !ok || info != (Info{Format: "png", Width: 30, Height: 20}) {
		t.Fatalf("Probe = %+v, %v", info, ok)
	}
	if got := info.String(); got != "PNG 30 × 20" {
		t.Fatalf("String = %q", got)
	}
	if _, ok := Probe([]byte("plain text\n")); ok {
		t.Fatal("text was recognized as an image")
	}
}

func TestDecodeScalesDown(t *testing.T) {
	img, info, err := Decode(encodePNG(t, 400, 100), 200)
	if err != nil {
		t.Fatal(err)
	}
	if info.Width != 400 || info.Height != 100 {
		t.Fatalf("info = %+v", info)
	}
	if got := img.Bounds().Size(); got != image.Pt(200, 50) {
		t.Fatalf("decoded size = %v", got)
	}
}

func TestDecodeRejectsHugeDimensionsBeforeDecoding(t *testing.T) {
	data := encodePNG(t, 1, 1)
	// IHDR starts after the 8-byte signature, 4-byte length and 4-byte type.
	binary.BigEndian.PutUint32(data[16:], 10000)
	binary.BigEndian.PutUint32(data[20:], 10000)
	binary.BigEndian.PutUint32(data[29:], crc32.ChecksumIEEE(data[12:29]))

	_, info, err := Decode(data, 100)
	if !errors.Is(err, ErrTooManyPixels) {
		t.Fatalf("err = %v", err)
	}
	if info.Width != 10000 {
		t.Fatalf("info = %+v", info)
	}
}

func TestScaleKeepsAspectRatio(t *testing.T) {
	small := image.NewRGBA(image.Rect(0, 0, 10, 5))
	if got := Scale(small, 40, 40).Bounds().Size(); got != image.Pt(40, 20) {
		t.Fatalf("enlarged size = %v", got)
	}
	if got := Fit(small, 40, 40); got != image.Image(small) {
		t.Fatal("Fit enlarged a small image")
	}
}

func TestHasImageExtension(t *testing.T) {
	for path, want := range map[string]bool{
		"logo.PNG": true, "a/b/photo.jpeg": true, "icon.webp": true,
		"drawing.svg": false, "notes.txt": false, "png": false,
	} {
		if got := HasImageExtension(path); got != want {
			t.Errorf("HasImageExtension(%q) = %v", path, got)
		}
	}
}
