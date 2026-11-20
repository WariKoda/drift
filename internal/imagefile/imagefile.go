// Package imagefile recognizes raster images and prepares them for display.
// It reads no files itself: callers pass the bytes they already loaded.
package imagefile

import (
	"bytes"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"path/filepath"
	"strings"

	_ "golang.org/x/image/bmp"
	"golang.org/x/image/draw"
	_ "golang.org/x/image/tiff"
	_ "golang.org/x/image/webp"
)

const (
	// MaxBytes limits how much of a file is read for decoding.
	MaxBytes = 20 << 20
	// MaxPixels rejects images whose decoded form would need too much memory.
	MaxPixels = 40_000_000
)

var (
	ErrNotImage      = errors.New("not a supported image")
	ErrTooManyPixels = fmt.Errorf("image exceeds %d megapixels", MaxPixels/1_000_000)
)

var extensions = map[string]struct{}{
	".png": {}, ".jpg": {}, ".jpeg": {}, ".gif": {}, ".webp": {},
	".bmp": {}, ".tif": {}, ".tiff": {},
}

// HasImageExtension reports whether path names a format this package decodes.
// It only decides how much of a file to read; content decides the format.
func HasImageExtension(path string) bool {
	_, ok := extensions[strings.ToLower(filepath.Ext(path))]
	return ok
}

// Info describes an image without its pixels.
type Info struct {
	Format string // png, jpeg, gif, webp, bmp, tiff
	Width  int
	Height int
}

// String renders the info for a status line, e.g. "PNG 1200 × 800".
func (i Info) String() string {
	return fmt.Sprintf("%s %d × %d", strings.ToUpper(i.Format), i.Width, i.Height)
}

// Probe reads the image header. It needs only the start of the file.
func Probe(data []byte) (Info, bool) {
	config, format, err := image.DecodeConfig(bytes.NewReader(data))
	if err != nil || config.Width <= 0 || config.Height <= 0 {
		return Info{}, false
	}
	return Info{Format: format, Width: config.Width, Height: config.Height}, true
}

// Decode decodes data and scales the result down so that neither side exceeds
// maxSide. Images above MaxPixels are rejected before their pixels are decoded.
func Decode(data []byte, maxSide int) (image.Image, Info, error) {
	info, ok := Probe(data)
	if !ok {
		return nil, Info{}, ErrNotImage
	}
	if int64(info.Width)*int64(info.Height) > MaxPixels {
		return nil, info, ErrTooManyPixels
	}
	decoded, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		return nil, info, fmt.Errorf("decode %s: %w", info.Format, err)
	}
	return Fit(decoded, maxSide, maxSide), info, nil
}

// Fit scales img down, keeping its aspect ratio, until it fits into
// maxWidth × maxHeight. Smaller images are returned unchanged.
func Fit(img image.Image, maxWidth, maxHeight int) image.Image {
	bounds := img.Bounds()
	width, height := bounds.Dx(), bounds.Dy()
	if width <= maxWidth && height <= maxHeight || maxWidth <= 0 || maxHeight <= 0 {
		return img
	}
	return Scale(img, maxWidth, maxHeight)
}

// Stretch resizes img to exactly width × height, for pixels that are not
// square on screen.
func Stretch(img image.Image, width, height int) image.Image {
	bounds := img.Bounds()
	if bounds.Dx() <= 0 || bounds.Dy() <= 0 || width <= 0 || height <= 0 || width == bounds.Dx() && height == bounds.Dy() {
		return img
	}
	target := image.Rect(0, 0, width, height)
	scaled := image.NewRGBA(target)
	draw.BiLinear.Scale(scaled, target, img, bounds, draw.Src, nil)
	return scaled
}

// Scale resizes img to fit into width × height, keeping its aspect ratio.
// Unlike Fit it also enlarges.
func Scale(img image.Image, width, height int) image.Image {
	bounds := img.Bounds()
	if bounds.Dx() <= 0 || bounds.Dy() <= 0 || width <= 0 || height <= 0 {
		return img
	}
	scale := min(float64(width)/float64(bounds.Dx()), float64(height)/float64(bounds.Dy()))
	target := image.Rect(0, 0, max(1, int(float64(bounds.Dx())*scale)), max(1, int(float64(bounds.Dy())*scale)))
	if target.Dx() == bounds.Dx() && target.Dy() == bounds.Dy() {
		return img
	}
	scaled := image.NewRGBA(target)
	draw.BiLinear.Scale(scaled, target, img, bounds, draw.Src, nil)
	return scaled
}
