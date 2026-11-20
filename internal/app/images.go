package app

import (
	"errors"
	"fmt"
	"image"
	"io"

	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/imagefile"
	"github.com/WariKoda/drift/internal/remote"
)

// ImageSide is one side of an image comparison prepared for display.
type ImageSide struct {
	Missing bool // the file does not exist on this side
	Info    imagefile.Info
	Image   image.Image // scaled down for display; nil when Err is set
	Err     error
}

// ImagePair holds both sides of an image comparison.
type ImagePair struct {
	Local, Remote ImageSide
	// SamePixels reports equal dimensions and pixels although the bytes
	// differ, e.g. after re-encoding or a metadata change.
	SamePixels bool
}

// LoadImagePair reads and decodes both files of a comparison for display. It
// reports problems per side instead of failing, so one unreadable side still
// shows the other. Remote reads are not cancelled: interrupting them would
// require closing the connection the diff view keeps for syncing.
func LoadImagePair(root *fs.Root, conn remote.Client, localPath, remotePath string, localMissing, remoteMissing bool, maxSide int) ImagePair {
	var pair ImagePair
	var local, remote image.Image
	pair.Local.Missing, pair.Remote.Missing = localMissing, remoteMissing
	if !localMissing {
		local = loadImageSide(&pair.Local, func() (io.ReadCloser, error) { return root.Open(localPath) })
	}
	if !remoteMissing {
		remote = loadImageSide(&pair.Remote, func() (io.ReadCloser, error) { return conn.Open(remotePath) })
	}
	pair.SamePixels = local != nil && remote != nil && samePixels(local, remote)
	pair.Local.Image = fitSide(local, maxSide)
	pair.Remote.Image = fitSide(remote, maxSide)
	return pair
}

// loadImageSide fills side and returns the full-resolution image for the
// pixel comparison.
func loadImageSide(side *ImageSide, open func() (io.ReadCloser, error)) image.Image {
	reader, err := open()
	if err != nil {
		side.Err = err
		return nil
	}
	data, readErr := io.ReadAll(io.LimitReader(reader, imagefile.MaxBytes+1))
	if err := errors.Join(readErr, reader.Close()); err != nil {
		side.Err = err
		return nil
	}
	if len(data) > imagefile.MaxBytes {
		side.Err = fmt.Errorf("file exceeds %d MiB", imagefile.MaxBytes>>20)
		return nil
	}
	// Decode at full size: scaling first could hide a small change.
	img, info, err := imagefile.Decode(data, 0)
	side.Info, side.Err = info, err
	return img
}

func fitSide(img image.Image, maxSide int) image.Image {
	if img == nil {
		return nil
	}
	return imagefile.Fit(img, maxSide, maxSide)
}

func samePixels(a, b image.Image) bool {
	boundsA, boundsB := a.Bounds(), b.Bounds()
	if boundsA.Size() != boundsB.Size() {
		return false
	}
	for y := range boundsA.Dy() {
		for x := range boundsA.Dx() {
			r1, g1, b1, a1 := a.At(boundsA.Min.X+x, boundsA.Min.Y+y).RGBA()
			r2, g2, b2, a2 := b.At(boundsB.Min.X+x, boundsB.Min.Y+y).RGBA()
			if r1 != r2 || g1 != g2 || b1 != b2 || a1 != a2 {
				return false
			}
		}
	}
	return true
}
