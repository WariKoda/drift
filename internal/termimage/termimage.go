// Package termimage shows images in the terminal as ordinary text rows.
//
// With the Kitty graphics protocol an image is transmitted once, outside the
// Bubble Tea frame, through the same Output the renderer writes to. The view
// then draws Unicode placeholder characters whose foreground colour carries the
// image ID and whose combining marks carry the row and column. Terminals
// without that protocol but with true colour get half blocks instead: each cell
// is "▀" with the upper pixel as foreground and the lower as background. Either
// way the line renderer can skip, rewrite and truncate the rows like any other
// text.
package termimage

import (
	"bytes"
	"compress/zlib"
	"encoding/base64"
	"fmt"
	"image"
	"image/color"
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

// EnvVar overrides detection: "off", "kitty" or "blocks".
const EnvVar = "DRIFT_IMAGES"

// maxUpscale limits how far a small image is enlarged to fill its area. Half
// blocks are far larger than screen pixels, so they never enlarge.
const (
	maxUpscale       = 2
	maxBlocksUpscale = 1
)

// maxCells is the number of rows and columns placeholders can address.
const maxCells = 256

// Cell size assumed when the terminal does not report pixel dimensions.
const (
	defaultCellWidth  = 10
	defaultCellHeight = 20
)

// Mode is how images are drawn.
type Mode int

const (
	Off    Mode = iota
	Kitty       // Kitty graphics protocol with Unicode placeholders
	Blocks      // half blocks in true colour
)

func (m Mode) String() string {
	switch m {
	case Kitty:
		return "kitty"
	case Blocks:
		return "blocks"
	default:
		return "off"
	}
}

// Detection explains how images are drawn and why.
type Detection struct {
	Mode   Mode
	Tmux   bool
	Reason string
}

// Detect decides from the environment how to draw images. Kitty and Ghostty
// get the Kitty protocol; other terminals with true colour get half blocks.
// Inside tmux the outer terminal is unknown, so the Kitty protocol needs
// DRIFT_IMAGES=kitty and allow-passthrough on.
func Detect(getenv func(string) string, trueColor bool) Detection {
	tmux := getenv("TMUX") != ""
	switch strings.ToLower(strings.TrimSpace(getenv(EnvVar))) {
	case "off", "0", "false", "no":
		return Detection{Tmux: tmux, Reason: EnvVar + " turns images off"}
	case "kitty":
		return Detection{Mode: Kitty, Tmux: tmux, Reason: EnvVar + " forces the Kitty graphics protocol"}
	case "blocks":
		return Detection{Mode: Blocks, Tmux: tmux, Reason: EnvVar + " forces half blocks"}
	}
	if !tmux {
		switch {
		case getenv("TERM") == "xterm-kitty" || getenv("KITTY_WINDOW_ID") != "":
			return Detection{Mode: Kitty, Reason: "Kitty detected"}
		case getenv("TERM") == "xterm-ghostty" || strings.EqualFold(getenv("TERM_PROGRAM"), "ghostty"):
			return Detection{Mode: Kitty, Reason: "Ghostty detected"}
		}
	}
	reason := "no Kitty graphics protocol"
	if tmux {
		reason = "tmux hides the outer terminal; " + EnvVar + "=kitty with allow-passthrough on draws pixels"
	}
	if trueColor {
		return Detection{Mode: Blocks, Tmux: tmux, Reason: reason + ", half blocks in true colour"}
	}
	return Detection{Tmux: tmux, Reason: reason + " and no true colour"}
}

// Placement is an image drawn in cells. Kitty placements live in the terminal
// under ID; half-block placements carry their text.
type Placement struct {
	ID   uint32
	Cols int
	Rows int
	text []string
}

// Output is the program's terminal writer. Bubble Tea renders through it, and
// image transmissions take the same lock, so a transmission never lands in the
// middle of a frame.
type Output struct {
	mu        sync.Mutex
	file      *os.File
	detection Detection
	mode      Mode
	tmux      bool
	nextID    uint32
	shown     map[uint32]struct{}
}

// NewOutput wraps file and draws images in the detected mode.
func NewOutput(file *os.File, detection Detection) *Output {
	// Start at a varying ID so that images of an earlier run, still referenced
	// by placeholders in the scrollback, are not replaced by this run's.
	start := uint32(time.Now().UnixNano())&0x7fffff | 1
	return &Output{file: file, detection: detection, mode: detection.Mode, tmux: detection.Tmux, nextID: start, shown: map[uint32]struct{}{}}
}

// Detection reports how images were turned on or off.
func (o *Output) Detection() Detection { return o.detection }

// Enabled reports whether images can be shown. A nil Output has no images.
func (o *Output) Enabled() bool { return o != nil && o.mode != Off }

// cell returns the pixel size of one cell and the largest enlargement. Half
// blocks have two square pixels per cell.
func (o *Output) cell() (int, int, float64) {
	if o.mode == Blocks {
		return 1, 2, maxBlocksUpscale
	}
	width, height := cellSize(o.file)
	return width, height, maxUpscale
}

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

// Show places img into an area of at most maxCols × maxRows cells, keeping
// its aspect ratio. With the Kitty protocol it transmits the image first.
func (o *Output) Show(img image.Image, maxCols, maxRows int) (Placement, error) {
	if !o.Enabled() {
		return Placement{}, fmt.Errorf("terminal images are off")
	}
	cellWidth, cellHeight, upscale := o.cell()
	cols, rows, width, height := fitBox(img.Bounds().Dx(), img.Bounds().Dy(), maxCols, maxRows, cellWidth, cellHeight, upscale)
	if cols == 0 || rows == 0 {
		return Placement{}, fmt.Errorf("no room for the image")
	}

	o.mu.Lock()
	id := o.nextID
	o.nextID = o.nextID%0xffffff + 1
	o.mu.Unlock()

	if o.mode == Blocks {
		return Placement{ID: id, Cols: cols, Rows: rows, text: halfBlocks(imagefile.Scale(img, width, height), cols, rows)}, nil
	}
	// The terminal fits the image into the cell box, so only shrink before sending.
	scaled := imagefile.Fit(img, width, height)

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

// Delete frees an image and its placements in the terminal. Half blocks hold
// nothing there, so only Kitty IDs are known.
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
	if p.text != nil {
		return p.text
	}
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
	cellWidth, cellHeight, factor := o.cell()
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
func fitBox(width, height, maxCols, maxRows, cellWidth, cellHeight int, upscale float64) (cols, rows, pixelWidth, pixelHeight int) {
	maxCols, maxRows = min(maxCols, maxCells), min(maxRows, maxCells)
	if width <= 0 || height <= 0 || maxCols <= 0 || maxRows <= 0 {
		return 0, 0, 0, 0
	}
	factor := min(float64(maxCols*cellWidth)/float64(width), float64(maxRows*cellHeight)/float64(height), upscale)
	pixelWidth = max(1, int(float64(width)*factor))
	pixelHeight = max(1, int(float64(height)*factor))
	cols = min(maxCols, max(1, (pixelWidth+cellWidth-1)/cellWidth))
	rows = min(maxRows, max(1, (pixelHeight+cellHeight-1)/cellHeight))
	return cols, rows, pixelWidth, pixelHeight
}

// halfBlocks draws img as cols × rows cells of "▀": the upper pixel is the
// foreground, the lower the background. Mostly transparent pixels keep the
// terminal background. A colour is only written when it changes.
func halfBlocks(img image.Image, cols, rows int) []string {
	bounds := img.Bounds()
	pixel := func(x, y int) (color.NRGBA, bool) {
		if x >= bounds.Dx() || y >= bounds.Dy() {
			return color.NRGBA{}, false
		}
		c := color.NRGBAModel.Convert(img.At(bounds.Min.X+x, bounds.Min.Y+y)).(color.NRGBA)
		return c, c.A >= 128
	}
	lines := make([]string, rows)
	var line strings.Builder
	for row := range rows {
		line.Reset()
		lastFg, lastBg := "", ""
		set := func(fg, bg string) {
			if fg == lastFg && bg == lastBg {
				return
			}
			line.WriteString("\x1b[0")
			line.WriteString(fg)
			line.WriteString(bg)
			line.WriteString("m")
			lastFg, lastBg = fg, bg
		}
		for col := range cols {
			upper, hasUpper := pixel(col, 2*row)
			lower, hasLower := pixel(col, 2*row+1)
			switch {
			case hasUpper && hasLower:
				set(";38;2;"+rgb(upper), ";48;2;"+rgb(lower))
				line.WriteString("▀")
			case hasUpper:
				set(";38;2;"+rgb(upper), "")
				line.WriteString("▀")
			case hasLower:
				set(";38;2;"+rgb(lower), "")
				line.WriteString("▄")
			default:
				set("", "")
				line.WriteString(" ")
			}
		}
		line.WriteString("\x1b[0m")
		lines[row] = line.String()
	}
	return lines
}

func rgb(c color.NRGBA) string {
	return fmt.Sprintf("%d;%d;%d", c.R, c.G, c.B)
}
