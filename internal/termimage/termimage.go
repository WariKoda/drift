// Package termimage shows images in the terminal with the Kitty graphics
// protocol and Unicode placeholders.
//
// An image is transmitted once, outside the Bubble Tea frame, through the same
// Output the renderer writes to. The view then draws it as ordinary text:
// placeholder characters whose foreground colour carries the image ID and whose
// combining marks carry the row and column. The line renderer can skip, rewrite
// and truncate those rows like any other text.
package termimage

import (
	"bytes"
	"compress/zlib"
	"encoding/base64"
	"fmt"
	"image"
	"image/draw"
	"io"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/WariKoda/drift/internal/imagefile"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/ansi/kitty"
)

// EnvVar overrides detection: "off" disables images, "kitty" forces them on.
const EnvVar = "DRIFT_IMAGES"

// maxUpscale limits how far a small image is enlarged to fill its area.
const maxUpscale = 2

// maxCells is the number of rows and columns placeholders can address.
const maxCells = 256

// Cell size assumed when the terminal does not report pixel dimensions.
const (
	defaultCellWidth  = 10
	defaultCellHeight = 20
)

// Detection explains whether images are on and why.
type Detection struct {
	Enabled bool
	Tmux    bool
	Reason  string
}

// Detect decides from the environment whether the terminal shows Kitty
// Unicode placeholders. Kitty and Ghostty are recognized. Inside tmux the outer
// terminal is unknown, so images stay off unless DRIFT_IMAGES=kitty and tmux
// has allow-passthrough enabled.
func Detect(getenv func(string) string) Detection {
	tmux := getenv("TMUX") != ""
	switch strings.ToLower(strings.TrimSpace(getenv(EnvVar))) {
	case "off", "0", "false", "no":
		return Detection{Tmux: tmux, Reason: EnvVar + " turns images off"}
	case "kitty", "on", "1", "true", "yes":
		return Detection{Enabled: true, Tmux: tmux, Reason: EnvVar + " forces the Kitty graphics protocol"}
	}
	if tmux {
		return Detection{Tmux: true, Reason: "tmux hides the outer terminal; set " + EnvVar + "=kitty and allow-passthrough on"}
	}
	switch {
	case getenv("TERM") == "xterm-kitty" || getenv("KITTY_WINDOW_ID") != "":
		return Detection{Enabled: true, Reason: "Kitty detected"}
	case getenv("TERM") == "xterm-ghostty" || strings.EqualFold(getenv("TERM_PROGRAM"), "ghostty"):
		return Detection{Enabled: true, Reason: "Ghostty detected"}
	}
	return Detection{Reason: "terminal without Kitty Unicode placeholders"}
}

// Placement is an image the terminal holds, sized in cells.
type Placement struct {
	ID   uint32
	Cols int
	Rows int
}

// Output is the program's terminal writer. Bubble Tea renders through it, and
// image transmissions take the same lock, so a transmission never lands in the
// middle of a frame.
type Output struct {
	mu        sync.Mutex
	file      *os.File
	detection Detection
	enabled   bool
	tmux      bool
	nextID    uint32
	shown     map[uint32]struct{}
}

// NewOutput wraps file. Images are only sent when detection enabled them.
func NewOutput(file *os.File, detection Detection) *Output {
	// Start at a varying ID so that images of an earlier run, still referenced
	// by placeholders in the scrollback, are not replaced by this run's.
	start := uint32(time.Now().UnixNano())&0x7fffff | 1
	return &Output{file: file, detection: detection, enabled: detection.Enabled, tmux: detection.Tmux, nextID: start, shown: map[uint32]struct{}{}}
}

// Detection reports how images were turned on or off.
func (o *Output) Detection() Detection { return o.detection }

// Enabled reports whether images can be shown. A nil Output has no images.
func (o *Output) Enabled() bool { return o != nil && o.enabled }

func (o *Output) Write(p []byte) (int, error) {
	o.mu.Lock()
	defer o.mu.Unlock()
	return o.file.Write(p)
}

// Read is required by Bubble Tea's terminal file interface; input is read from
// stdin, not from here.
func (o *Output) Read(p []byte) (int, error) { return o.file.Read(p) }

// Close leaves the terminal open; the process owns stdout.
func (o *Output) Close() error { return nil }

// Fd lets Bubble Tea query the terminal size and mode.
func (o *Output) Fd() uintptr { return o.file.Fd() }

// Show transmits img and places it into an area of at most maxCols × maxRows
// cells. The image keeps its aspect ratio; the box grows at most twofold.
func (o *Output) Show(img image.Image, maxCols, maxRows int) (Placement, error) {
	if !o.Enabled() {
		return Placement{}, fmt.Errorf("terminal images are off")
	}
	cellWidth, cellHeight := cellSize(o.file)
	cols, rows, width, height := fitBox(img.Bounds().Dx(), img.Bounds().Dy(), maxCols, maxRows, cellWidth, cellHeight)
	if cols == 0 || rows == 0 {
		return Placement{}, fmt.Errorf("no room for the image")
	}
	// The terminal fits the image into the cell box, so only shrink before sending.
	scaled := imagefile.Fit(img, width, height)

	o.mu.Lock()
	id := o.nextID
	o.nextID = o.nextID%0xffffff + 1
	o.mu.Unlock()

	sequence, err := o.transmission(id, scaled, cols, rows)
	if err != nil {
		return Placement{}, err
	}

	o.mu.Lock()
	defer o.mu.Unlock()
	if _, err := o.file.Write(sequence); err != nil {
		return Placement{}, fmt.Errorf("write image: %w", err)
	}
	o.shown[id] = struct{}{}
	return Placement{ID: id, Cols: cols, Rows: rows}, nil
}

// Delete frees an image and its placements in the terminal.
func (o *Output) Delete(id uint32) error {
	if !o.Enabled() || id == 0 {
		return nil
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	if _, ok := o.shown[id]; !ok {
		return nil
	}
	delete(o.shown, id)
	_, err := io.WriteString(o.file, o.wrap(ansi.KittyGraphics(nil, "a=d", "d=I", fmt.Sprintf("i=%d", id), "q=2")))
	return err
}

// Release frees every image this Output transmitted. Call it once the program
// has left the alternate screen.
func (o *Output) Release() error {
	if !o.Enabled() {
		return nil
	}
	o.mu.Lock()
	ids := make([]uint32, 0, len(o.shown))
	for id := range o.shown {
		ids = append(ids, id)
	}
	o.mu.Unlock()
	for _, id := range ids {
		if err := o.Delete(id); err != nil {
			return err
		}
	}
	return nil
}

func (o *Output) wrap(sequence string) string {
	if o.tmux {
		return ansi.TmuxPassthrough(sequence)
	}
	return sequence
}

// transmission encodes img as straight-alpha RGBA, compresses it and splits it
// into protocol chunks, followed by a virtual placement of cols × rows cells.
func (o *Output) transmission(id uint32, img image.Image, cols, rows int) ([]byte, error) {
	bounds := img.Bounds()
	pixels := image.NewNRGBA(image.Rect(0, 0, bounds.Dx(), bounds.Dy()))
	draw.Draw(pixels, pixels.Bounds(), img, bounds.Min, draw.Src)

	var compressed bytes.Buffer
	writer := zlib.NewWriter(&compressed)
	if _, err := writer.Write(pixels.Pix); err != nil {
		return nil, fmt.Errorf("compress image: %w", err)
	}
	if err := writer.Close(); err != nil {
		return nil, fmt.Errorf("compress image: %w", err)
	}
	payload := base64.StdEncoding.EncodeToString(compressed.Bytes())

	var sequence bytes.Buffer
	for offset := 0; offset == 0 || offset < len(payload); offset += kitty.MaxChunkSize {
		chunk := payload[offset:min(offset+kitty.MaxChunkSize, len(payload))]
		more := "m=0"
		if offset+kitty.MaxChunkSize < len(payload) {
			more = "m=1"
		}
		// Later chunks may only carry m and q.
		options := []string{more, "q=2"}
		if offset == 0 {
			options = append([]string{"a=t", "f=32", "o=z", fmt.Sprintf("s=%d", bounds.Dx()), fmt.Sprintf("v=%d", bounds.Dy()), fmt.Sprintf("i=%d", id)}, options...)
		}
		sequence.WriteString(o.wrap(ansi.KittyGraphics([]byte(chunk), options...)))
	}
	sequence.WriteString(o.wrap(ansi.KittyGraphics(nil, "a=p", "U=1", fmt.Sprintf("i=%d", id), fmt.Sprintf("c=%d", cols), fmt.Sprintf("r=%d", rows), "q=2")))
	return sequence.Bytes(), nil
}

// Rows returns one string per cell row. Each row is exactly p.Cols cells wide.
func Rows(p Placement) []string {
	colour := fmt.Sprintf("\x1b[38;2;%d;%d;%dm", p.ID>>16&0xff, p.ID>>8&0xff, p.ID&0xff)
	rows := make([]string, p.Rows)
	var line strings.Builder
	for row := range p.Rows {
		line.Reset()
		line.WriteString(colour)
		for col := range p.Cols {
			line.WriteRune(kitty.Placeholder)
			line.WriteRune(kitty.Diacritic(row))
			line.WriteRune(kitty.Diacritic(col))
		}
		line.WriteString("\x1b[39m")
		rows[row] = line.String()
	}
	return rows
}

// CommonBoxes sizes images of the given pixel sizes with one scale for all,
// so their boxes keep their relative size. Each box fits into maxCols ×
// maxRows cells; pass a box to Show to place the image in it.
func (o *Output) CommonBoxes(sizes []image.Point, maxCols, maxRows int) []image.Point {
	cellWidth, cellHeight := cellSize(o.file)
	factor := float64(maxUpscale)
	for _, size := range sizes {
		if size.X > 0 && size.Y > 0 {
			factor = min(factor, float64(min(maxCols, maxCells)*cellWidth)/float64(size.X), float64(min(maxRows, maxCells)*cellHeight)/float64(size.Y))
		}
	}
	boxes := make([]image.Point, len(sizes))
	for index, size := range sizes {
		if size.X <= 0 || size.Y <= 0 || maxCols <= 0 || maxRows <= 0 {
			continue
		}
		width, height := max(1, int(float64(size.X)*factor)), max(1, int(float64(size.Y)*factor))
		boxes[index] = image.Pt(
			min(maxCols, max(1, (width+cellWidth-1)/cellWidth)),
			min(maxRows, max(1, (height+cellHeight-1)/cellHeight)),
		)
	}
	return boxes
}

// fitBox sizes an image of width × height pixels for an area of at most
// maxCols × maxRows cells. It returns the cell box and the pixel size to send.
func fitBox(width, height, maxCols, maxRows, cellWidth, cellHeight int) (cols, rows, pixelWidth, pixelHeight int) {
	maxCols, maxRows = min(maxCols, maxCells), min(maxRows, maxCells)
	if width <= 0 || height <= 0 || maxCols <= 0 || maxRows <= 0 {
		return 0, 0, 0, 0
	}
	factor := min(float64(maxCols*cellWidth)/float64(width), float64(maxRows*cellHeight)/float64(height), maxUpscale)
	pixelWidth = max(1, int(float64(width)*factor))
	pixelHeight = max(1, int(float64(height)*factor))
	cols = min(maxCols, max(1, (pixelWidth+cellWidth-1)/cellWidth))
	rows = min(maxRows, max(1, (pixelHeight+cellHeight-1)/cellHeight))
	return cols, rows, pixelWidth, pixelHeight
}
