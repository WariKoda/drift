package browser

import (
	"bytes"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/termimage"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/ansi/kitty"
)

func writeTestPNG(t *testing.T, path string, width, height int) []byte {
	t.Helper()
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for y := range height {
		for x := range width {
			img.SetNRGBA(x, y, color.NRGBA{R: uint8(x), G: uint8(y), B: 90, A: 255})
		}
	}
	var buffer bytes.Buffer
	if err := png.Encode(&buffer, img); err != nil {
		t.Fatal(err)
	}
	if path != "" {
		if err := os.WriteFile(path, buffer.Bytes(), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	return buffer.Bytes()
}

// terminalFile stands in for the terminal: everything drift sends ends up in
// a real file that the test reads back.
func terminalFile(t *testing.T) (*termimage.Output, func() string) {
	t.Helper()
	file, err := os.CreateTemp(t.TempDir(), "terminal")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { file.Close() })
	return termimage.NewOutput(file, termimage.Detection{Enabled: true}), func() string {
		data, err := os.ReadFile(file.Name())
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}
}

// runCmd executes cmd and any batch it returns, collecting the messages.
func runCmd(cmd tea.Cmd) []tea.Msg {
	if cmd == nil {
		return nil
	}
	msg := cmd()
	if batch, ok := msg.(tea.BatchMsg); ok {
		var messages []tea.Msg
		for _, inner := range batch {
			messages = append(messages, runCmd(inner)...)
		}
		return messages
	}
	if msg == nil {
		return nil
	}
	return []tea.Msg{msg}
}

func applyImageResults(t *testing.T, model *Model, cmd tea.Cmd) {
	t.Helper()
	for _, msg := range runCmd(cmd) {
		if shown, ok := msg.(msgPreviewImageShown); ok {
			runCmd(model.applyPreviewImageShown(shown))
		}
	}
}

func freed(id uint32) string {
	return ansi.KittyGraphics(nil, "a=d", "d=I", fmt.Sprintf("i=%d", id), "q=2")
}

func TestImagePreviewPlacesResizesAndFreesTheImage(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "logo.png")
	writeTestPNG(t, path, 200, 100)
	output, terminal := terminalFile(t)

	model, err := New(dir)
	if err != nil {
		t.Fatal(err)
	}
	model.SetImages(output)
	model.SetSize(80, 24)
	model.preview = filePreview{active: true, source: PaneLocal, generation: 1}
	request := previewRequest{generation: 1, source: PaneLocal, path: path, size: 1, session: model.remoteSession, decodeImages: true}

	loaded := readLocalPreviewCmd(request)().(msgPreviewLoaded)
	if loaded.err != nil || loaded.picture == nil || loaded.picture.image == nil {
		t.Fatalf("loaded = %+v", loaded)
	}
	applyImageResults(t, &model, model.applyPreviewLoaded(loaded))

	first := model.preview.picture.placement
	if first.ID == 0 {
		t.Fatal("image was not placed")
	}
	if sent := terminal(); !strings.Contains(sent, "a=t,f=32") || !strings.Contains(sent, fmt.Sprintf("a=p,U=1,i=%d", first.ID)) {
		t.Fatal("image was not transmitted and placed")
	}
	view := model.View()
	if !strings.Contains(ansi.Strip(view), "PNG 200 × 100") || !strings.ContainsRune(view, kitty.Placeholder) {
		t.Fatalf("preview lacks info line or placeholders:\n%s", ansi.Strip(view))
	}
	for index, line := range strings.Split(view, "\n") {
		if width := ansi.StringWidth(line); width != 80 {
			t.Fatalf("line %d is %d cells wide", index, width)
		}
	}
	if cmd := model.showPreviewImage(); cmd != nil {
		t.Fatal("unchanged pane size placed the image again")
	}

	applyImageResults(t, &model, model.SetSize(100, 30))
	second := model.preview.picture.placement
	if second.ID == 0 || second.ID == first.ID {
		t.Fatalf("resize placement = %+v, first = %+v", second, first)
	}
	if !strings.Contains(terminal(), freed(first.ID)) {
		t.Fatal("resize kept the old image in the terminal")
	}

	runCmd(model.disablePreview())
	if !strings.Contains(terminal(), freed(second.ID)) {
		t.Fatal("closing the preview kept the image in the terminal")
	}
}

func TestImagePreviewFreesSupersededPlacements(t *testing.T) {
	output, terminal := terminalFile(t)
	model := previewTestModel(nil)
	model.SetImages(output)
	model.preview = filePreview{active: true, source: PaneLocal, generation: 3, loaded: true,
		picture: &previewPicture{image: image.NewNRGBA(image.Rect(0, 0, 8, 8)), token: 2}}

	placement, err := output.Show(image.NewNRGBA(image.Rect(0, 0, 8, 8)), 4, 4)
	if err != nil {
		t.Fatal(err)
	}
	runCmd(model.applyPreviewImageShown(msgPreviewImageShown{generation: 3, token: 1, placement: placement}))
	if model.preview.picture.placement.ID != 0 || !strings.Contains(terminal(), freed(placement.ID)) {
		t.Fatal("a superseded placement was kept")
	}
}

func TestImagePreviewWithoutTerminalSupportShowsTheFormat(t *testing.T) {
	data := writeTestPNG(t, "", 64, 32)
	lines, picture, err := readPreview(bytes.NewReader(data), int64(len(data)), previewRequest{path: "/project/icon.png"})
	if err != nil || lines != nil || picture == nil || picture.image != nil {
		t.Fatalf("readPreview = %v, %+v, %v", lines, picture, err)
	}

	model := previewTestModel(nil)
	model.preview = filePreview{active: true, source: PaneLocal, loaded: true, path: "/project/icon.png", picture: picture}
	view := ansi.Strip(model.View())
	if !strings.Contains(view, "PNG 64 × 32") || !strings.Contains(view, "Inline images need Kitty or Ghostty") {
		t.Fatalf("preview without image support:\n%s", view)
	}
	if strings.Contains(view, "binary file") {
		t.Fatalf("image reported as binary file:\n%s", view)
	}
}

func TestImagePreviewShowsDecodeFailures(t *testing.T) {
	data := writeTestPNG(t, "", 64, 32)
	corrupt := append(data[:len(data)/2:len(data)/2], make([]byte, 16)...)
	_, picture, err := readPreview(bytes.NewReader(corrupt), int64(len(corrupt)), previewRequest{path: "/project/icon.png", decodeImages: true})
	if err != nil || picture == nil || picture.err == nil {
		t.Fatalf("readPreview = %+v, %v", picture, err)
	}
	model := previewTestModel(nil)
	output, _ := terminalFile(t)
	model.SetImages(output)
	model.preview = filePreview{active: true, source: PaneLocal, loaded: true, picture: picture}
	if cmd := model.showPreviewImage(); cmd != nil {
		t.Fatal("tried to place an image that failed to decode")
	}
	if view := ansi.Strip(model.View()); !strings.Contains(view, "Image cannot be shown: decode png") {
		t.Fatalf("decode failure missing:\n%s", view)
	}
}

func TestImagePreviewSizeLimit(t *testing.T) {
	const size = 5 << 20
	for _, tc := range []struct {
		path   string
		decode bool
		want   bool
	}{
		{"/p/photo.jpg", true, true},
		{"/p/photo.jpg", false, false},
		{"/p/notes.txt", true, false},
	} {
		if got := previewSizeAllowed(previewRequest{path: tc.path, decodeImages: tc.decode}, size); got != tc.want {
			t.Errorf("previewSizeAllowed(%s, decode=%v) = %v", tc.path, tc.decode, got)
		}
	}
}

func TestImagePreviewOffersOnlyApplicableKeys(t *testing.T) {
	data := writeTestPNG(t, "", 64, 32)
	_, picture, err := readPreview(bytes.NewReader(data), int64(len(data)), previewRequest{path: "/project/icon.png"})
	if err != nil {
		t.Fatal(err)
	}
	model := previewTestModel(nil)
	model.Width = 160
	model.preview = filePreview{active: true, source: PaneLocal, loaded: true, path: "/project/icon.png", picture: picture}
	status := ansi.Strip(strings.Split(model.View(), "\n")[model.Height-1])
	if !strings.Contains(status, "[p]close") || !strings.Contains(status, "[?]help") {
		t.Fatalf("status lacks close or help: %q", status)
	}
	for _, unwanted := range []string{"copy", "select", "scroll", "jump"} {
		if strings.Contains(status, unwanted) {
			t.Fatalf("status offers %q for an image: %q", unwanted, status)
		}
	}

	model.preview = filePreview{active: true, source: PaneLocal, loaded: true, path: "/project/a.txt", lines: []string{"text"}}
	model.layoutPreview(false)
	if status := ansi.Strip(strings.Split(model.View(), "\n")[model.Height-1]); !strings.Contains(status, "[c]copy") {
		t.Fatalf("text preview lost its copy hint: %q", status)
	}
}
