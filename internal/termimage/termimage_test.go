package termimage

import (
	"bytes"
	"compress/zlib"
	"encoding/base64"
	"image"
	"image/color"
	"io"
	"os"
	"regexp"
	"strconv"
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/ansi/kitty"
)

func env(values map[string]string) func(string) string {
	return func(key string) string { return values[key] }
}

func TestDetect(t *testing.T) {
	for _, tc := range []struct {
		name      string
		values    map[string]string
		trueColor bool
		mode      Mode
		tmux      bool
	}{
		{"kitty", map[string]string{"TERM": "xterm-kitty"}, true, Kitty, false},
		{"ghostty term", map[string]string{"TERM": "xterm-ghostty"}, true, Kitty, false},
		{"ghostty program", map[string]string{"TERM": "xterm-256color", "TERM_PROGRAM": "ghostty"}, true, Kitty, false},
		{"foot", map[string]string{"TERM": "foot", "COLORTERM": "truecolor"}, true, Blocks, false},
		{"256 colours", map[string]string{"TERM": "xterm-256color"}, false, Off, false},
		{"turned off", map[string]string{"TERM": "xterm-kitty", EnvVar: "off"}, true, Off, false},
		{"blocks forced in kitty", map[string]string{"TERM": "xterm-kitty", EnvVar: "blocks"}, true, Blocks, false},
		{"tmux inside ghostty", map[string]string{"TERM": "tmux-256color", "TMUX": "/tmp/tmux", "TERM_PROGRAM": "ghostty"}, true, Blocks, true},
		{"tmux forced", map[string]string{"TMUX": "/tmp/tmux", EnvVar: "kitty"}, false, Kitty, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got := Detect(env(tc.values), tc.trueColor)
			if got.Mode != tc.mode || got.Tmux != tc.tmux || got.Reason == "" {
				t.Fatalf("Detect = %+v", got)
			}
		})
	}
}

func TestFitBox(t *testing.T) {
	for _, tc := range []struct {
		name                 string
		width, height        int
		maxCols, maxRows     int
		cols, rows, pxW, pxH int
	}{
		{"wide image fills the width", 800, 200, 40, 30, 40, 5, 400, 100},
		{"tall image fills the height", 100, 1000, 40, 10, 2, 10, 20, 200},
		{"small image grows at most twofold", 20, 20, 40, 30, 4, 2, 40, 40},
		{"no room", 20, 20, 0, 30, 0, 0, 0, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cols, rows, pxW, pxH := fitBox(tc.width, tc.height, tc.maxCols, tc.maxRows, 10, 20, maxUpscale)
			if cols != tc.cols || rows != tc.rows || pxW != tc.pxW || pxH != tc.pxH {
				t.Fatalf("fitBox = %d×%d cells, %d×%d px", cols, rows, pxW, pxH)
			}
		})
	}
}

func TestRowsAreOneCellPerColumn(t *testing.T) {
	rows := Rows(Placement{ID: 0x0a0b0c, Cols: 7, Rows: 3})
	if len(rows) != 3 {
		t.Fatalf("rows = %d", len(rows))
	}
	for index, row := range rows {
		if width := ansi.StringWidth(row); width != 7 {
			t.Fatalf("row %d width = %d", index, width)
		}
		if !strings.HasPrefix(row, "\x1b[38;2;10;11;12m") {
			t.Fatalf("row %d does not carry the image ID: %q", index, row)
		}
		if !strings.Contains(row, string([]rune{kitty.Placeholder, kitty.Diacritic(index), kitty.Diacritic(6)})) {
			t.Fatalf("row %d lacks its last cell", index)
		}
	}
	// Truncation keeps whole cells, so a narrower pane shows a cropped image.
	if width := ansi.StringWidth(ansi.Truncate(rows[0], 4, "")); width != 4 {
		t.Fatalf("truncated width = %d", width)
	}
}

func newTestOutput(t *testing.T, detection Detection) (*Output, *os.File) {
	t.Helper()
	file, err := os.CreateTemp(t.TempDir(), "terminal")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { file.Close() })
	return NewOutput(file, detection), file
}

func written(t *testing.T, file *os.File) string {
	t.Helper()
	data, err := os.ReadFile(file.Name())
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

var apc = regexp.MustCompile("\x1b_G([^;\x1b]*)(?:;([^\x1b]*))?\x1b\\\\")

func TestShowTransmitsStraightAlphaAndPlaces(t *testing.T) {
	output, file := newTestOutput(t, Detection{Mode: Kitty})
	// Large enough to need several chunks after compression.
	img := image.NewNRGBA(image.Rect(0, 0, 120, 90))
	for y := range 90 {
		for x := range 120 {
			img.SetNRGBA(x, y, color.NRGBA{R: uint8(x * 7), G: uint8(y * 13), B: uint8(x ^ y), A: 255})
		}
	}
	img.SetNRGBA(0, 0, color.NRGBA{R: 200, G: 100, B: 50, A: 128})

	placement, err := output.Show(img, 1000, 1000)
	if err != nil {
		t.Fatal(err)
	}
	matches := apc.FindAllStringSubmatch(written(t, file), -1)
	if len(matches) < 3 {
		t.Fatalf("got %d sequences, want several chunks and a placement", len(matches))
	}
	first, place := matches[0][1], matches[len(matches)-1][1]
	for _, option := range []string{"a=t", "f=32", "o=z", "s=120", "v=90", "m=1", "q=2"} {
		if !strings.Contains(first, option) {
			t.Fatalf("first chunk %q lacks %s", first, option)
		}
	}
	for _, option := range []string{"a=p", "U=1", "c=", "r="} {
		if !strings.Contains(place, option) {
			t.Fatalf("placement %q lacks %s", place, option)
		}
	}
	if !strings.Contains(matches[len(matches)-2][1], "m=0") {
		t.Fatal("last chunk does not end the transmission")
	}

	var payload strings.Builder
	for _, match := range matches[:len(matches)-1] {
		payload.WriteString(match[2])
	}
	compressed, err := base64.StdEncoding.DecodeString(payload.String())
	if err != nil {
		t.Fatal(err)
	}
	reader, err := zlib.NewReader(bytes.NewReader(compressed))
	if err != nil {
		t.Fatal(err)
	}
	pixels, err := io.ReadAll(reader)
	if err != nil {
		t.Fatal(err)
	}
	if len(pixels) != 120*90*4 {
		t.Fatalf("decoded %d bytes", len(pixels))
	}
	if got := pixels[:4]; !bytes.Equal(got, []byte{200, 100, 50, 128}) {
		t.Fatalf("semi-transparent pixel = %v, want straight alpha", got)
	}
	if placement.ID == 0 || placement.Cols == 0 || placement.Rows == 0 {
		t.Fatalf("placement = %+v", placement)
	}
}

func TestDeleteOnlyFreesShownImages(t *testing.T) {
	output, file := newTestOutput(t, Detection{Mode: Kitty})
	placement, err := output.Show(image.NewNRGBA(image.Rect(0, 0, 4, 4)), 10, 10)
	if err != nil {
		t.Fatal(err)
	}
	before := len(written(t, file))
	if err := output.Delete(placement.ID + 1); err != nil {
		t.Fatal(err)
	}
	if len(written(t, file)) != before {
		t.Fatal("deleted an image this output never sent")
	}
	if err := output.Release(); err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(written(t, file), ansi.KittyGraphics(nil, "a=d", "d=I", "i="+strconv.FormatUint(uint64(placement.ID), 10), "q=2")) {
		t.Fatal("release did not free the image")
	}
	if err := output.Delete(placement.ID); err != nil {
		t.Fatal(err)
	}
}

func TestTmuxWrapsEverySequence(t *testing.T) {
	output, file := newTestOutput(t, Detection{Mode: Kitty, Tmux: true})
	if _, err := output.Show(image.NewNRGBA(image.Rect(0, 0, 4, 4)), 10, 10); err != nil {
		t.Fatal(err)
	}
	text := written(t, file)
	if !strings.HasPrefix(text, "\x1bPtmux;\x1b\x1b_G") || strings.Count(text, "\x1bPtmux;") != 2 {
		t.Fatalf("not wrapped for tmux: %q", text)
	}
}

func TestDisabledOutputWritesNothing(t *testing.T) {
	output, file := newTestOutput(t, Detection{})
	if _, err := output.Show(image.NewNRGBA(image.Rect(0, 0, 4, 4)), 10, 10); err == nil {
		t.Fatal("Show succeeded with images off")
	}
	var nilOutput *Output
	if nilOutput.Enabled() || output.Release() != nil || written(t, file) != "" {
		t.Fatal("disabled output wrote to the terminal")
	}
}

func TestCommonBoxesKeepRelativeSizes(t *testing.T) {
	output, _ := newTestOutput(t, Detection{Mode: Kitty})
	// A temporary file reports no cell size, so cells are 10 × 20 pixels.
	boxes := output.CommonBoxes([]image.Point{{200, 100}, {100, 50}, {}}, 40, 30)
	want := []image.Point{{40, 10}, {20, 5}, {}}
	for index := range want {
		if boxes[index] != want[index] {
			t.Fatalf("boxes = %v, want %v", boxes, want)
		}
	}
}

func TestHalfBlocksDrawTwoPixelsPerCell(t *testing.T) {
	output, file := newTestOutput(t, Detection{Mode: Blocks})
	img := image.NewNRGBA(image.Rect(0, 0, 4, 4))
	for y := range 4 {
		for x := range 4 {
			img.SetNRGBA(x, y, color.NRGBA{R: uint8(10 * (y + 1)), G: uint8(x), B: 0, A: 255})
		}
	}
	// A transparent lower pixel keeps the terminal background.
	img.SetNRGBA(3, 3, color.NRGBA{})

	placement, err := output.Show(img, 10, 10)
	if err != nil {
		t.Fatal(err)
	}
	if placement.Cols != 4 || placement.Rows != 2 {
		t.Fatalf("placement = %d × %d cells, want 4 × 2 without enlarging", placement.Cols, placement.Rows)
	}
	rows := Rows(placement)
	for index, row := range rows {
		if width := ansi.StringWidth(row); width != 4 {
			t.Fatalf("row %d width = %d", index, width)
		}
		if !strings.HasSuffix(row, "\x1b[0m") {
			t.Fatalf("row %d does not reset its colours: %q", index, row)
		}
	}
	if !strings.HasPrefix(rows[0], "\x1b[0;38;2;10;0;0;48;2;20;0;0m▀") {
		t.Fatalf("first cell = %q", rows[0])
	}
	if !strings.HasSuffix(rows[1], "\x1b[0;38;2;30;3;0m▀\x1b[0m") {
		t.Fatalf("cell above a transparent pixel = %q", rows[1])
	}
	if written(t, file) != "" {
		t.Fatal("half blocks wrote to the terminal outside the frame")
	}
	if err := output.Delete(placement.ID); err != nil || output.Release() != nil || written(t, file) != "" {
		t.Fatal("freeing half blocks wrote to the terminal")
	}
}

func TestHalfBlocksShrinkToTheArea(t *testing.T) {
	output, _ := newTestOutput(t, Detection{Mode: Blocks})
	placement, err := output.Show(image.NewNRGBA(image.Rect(0, 0, 400, 100)), 40, 30)
	if err != nil {
		t.Fatal(err)
	}
	if placement.Cols != 40 || placement.Rows != 5 {
		t.Fatalf("placement = %d × %d cells, want 40 × 5", placement.Cols, placement.Rows)
	}
	boxes := output.CommonBoxes([]image.Point{{400, 100}, {200, 50}}, 40, 30)
	if boxes[0] != image.Pt(40, 5) || boxes[1] != image.Pt(20, 3) {
		t.Fatalf("boxes = %v", boxes)
	}
}
