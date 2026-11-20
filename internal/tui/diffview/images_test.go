package diffview

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

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/termimage"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/ansi/kitty"
	"golang.org/x/image/bmp"
)

func diffTestImage(width, height int, shade uint8) *image.NRGBA {
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for y := range height {
		for x := range width {
			img.SetNRGBA(x, y, color.NRGBA{R: uint8(x), G: uint8(y), B: shade, A: 255})
		}
	}
	return img
}

func pngBytes(t *testing.T, img image.Image) []byte {
	t.Helper()
	var buffer bytes.Buffer
	if err := png.Encode(&buffer, img); err != nil {
		t.Fatal(err)
	}
	return buffer.Bytes()
}

// imageDiffModel compares a local and a remote image over a real FTP server.
// images may be nil to model a terminal without image support.
func imageDiffModel(t *testing.T, local, remote []byte, images *termimage.Output) Model {
	t.Helper()
	server := ftptest.Start(t, 1)
	server.AddFile("/logo.png", string(remote))
	conn := connectDiffTestHost(t, server.Host(t))
	dir := t.TempDir()
	localPath := filepath.Join(dir, "logo.png")
	if err := os.WriteFile(localPath, local, 0o600); err != nil {
		t.Fatal(err)
	}
	root, err := fs.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { root.Close() })
	result, err := diff.Compare(root, localPath, "/logo.png", conn)
	if err != nil {
		t.Fatal(err)
	}
	sessions := []diff.Session{{LocalPath: localPath, RemotePath: "/logo.png", Result: result, Loaded: true}}
	model := New(sessions, server.Host(t), conn, root, 120, 40)
	model.SetImages(images)
	return model
}

func imageTerminal(t *testing.T) (*termimage.Output, func() string) {
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

// settle runs cmd and feeds every resulting message back into the model until
// no work is left.
func settle(model Model, cmd tea.Cmd) Model {
	queue := []tea.Cmd{cmd}
	for len(queue) > 0 {
		next := queue[0]
		queue = queue[1:]
		if next == nil {
			continue
		}
		switch msg := next().(type) {
		case nil:
		case tea.BatchMsg:
			queue = append(queue, msg...)
		default:
			var more tea.Cmd
			model, more = model.Update(msg)
			queue = append(queue, more)
		}
	}
	return model
}

func freedImage(id uint32) string {
	return ansi.KittyGraphics(nil, "a=d", "d=I", fmt.Sprintf("i=%d", id), "q=2")
}

func TestImageComparisonShowsBothSidesAndFreesThem(t *testing.T) {
	output, terminal := imageTerminal(t)
	model := imageDiffModel(t, pngBytes(t, diffTestImage(200, 100, 10)), pngBytes(t, diffTestImage(120, 60, 200)), output)
	model = settle(model, model.Init())

	picture := model.picture
	if picture == nil || picture.local.ID == 0 || picture.remote.ID == 0 {
		t.Fatalf("picture = %+v", picture)
	}
	if picture.local.Cols <= picture.remote.Cols {
		t.Fatalf("the larger local image got the smaller box: local %+v, remote %+v", picture.local, picture.remote)
	}
	if strings.Count(terminal(), "a=t,f=32") != 2 {
		t.Fatal("both sides were not transmitted")
	}
	view := model.View()
	text := ansi.Strip(view)
	for _, want := range []string{"Image file — no text diff", "PNG 200 × 100", "PNG 120 × 60", "local", "remote"} {
		if !strings.Contains(text, want) {
			t.Fatalf("view lacks %q:\n%s", want, text)
		}
	}
	if !strings.ContainsRune(view, kitty.Placeholder) {
		t.Fatal("view has no image placeholders")
	}
	for index, line := range strings.Split(view, "\n") {
		if width := ansi.StringWidth(line); width != 120 {
			t.Fatalf("line %d is %d cells wide", index, width)
		}
	}

	first := *picture
	model = settle(model, model.SetSize(140, 44))
	if model.picture.local.ID == first.local.ID {
		t.Fatal("resize did not place the images again")
	}
	if sent := terminal(); !strings.Contains(sent, freedImage(first.local.ID)) || !strings.Contains(sent, freedImage(first.remote.ID)) {
		t.Fatal("resize kept the old images in the terminal")
	}

	second := *model.picture
	settle(model, model.Close())
	if sent := terminal(); !strings.Contains(sent, freedImage(second.local.ID)) || !strings.Contains(sent, freedImage(second.remote.ID)) {
		t.Fatal("closing the diff kept the images in the terminal")
	}
}

func TestImageComparisonFreesPlacementsArrivingAfterClose(t *testing.T) {
	output, terminal := imageTerminal(t)
	model := imageDiffModel(t, pngBytes(t, diffTestImage(20, 20, 1)), pngBytes(t, diffTestImage(20, 20, 2)), output)
	result := model.sessions[0].Result
	placement, err := output.Show(diffTestImage(4, 4, 0), 2, 2)
	if err != nil {
		t.Fatal(err)
	}
	model.Close()
	model = settle(model, func() tea.Msg { return MsgImagesPlaced{result: result, token: 1, local: placement} })
	if !strings.Contains(terminal(), freedImage(placement.ID)) {
		t.Fatal("a placement finishing after Close was kept")
	}
}

func TestImageComparisonRecognizesReencodedImages(t *testing.T) {
	output, _ := imageTerminal(t)
	picture := diffTestImage(64, 32, 90)
	var remote bytes.Buffer
	if err := bmp.Encode(&remote, picture); err != nil {
		t.Fatal(err)
	}
	model := imageDiffModel(t, pngBytes(t, picture), remote.Bytes(), output)
	model = settle(model, model.Init())
	text := ansi.Strip(model.View())
	if !strings.Contains(text, "Same pixels, different bytes") || !strings.Contains(text, "BMP 64 × 32") {
		t.Fatalf("re-encoded image not reported:\n%s", text)
	}
}

func TestImageComparisonWithoutTerminalSupport(t *testing.T) {
	model := imageDiffModel(t, pngBytes(t, diffTestImage(30, 10, 1)), pngBytes(t, diffTestImage(30, 12, 2)), nil)
	if cmd := model.Init(); cmd != nil {
		model = settle(model, cmd)
	}
	if model.picture != nil {
		t.Fatal("loaded images for a terminal that cannot show them")
	}
	text := ansi.Strip(model.View())
	for _, want := range []string{"Image file — no text diff", "PNG 30 × 10", "PNG 30 × 12", "Inline images need Kitty or Ghostty"} {
		if !strings.Contains(text, want) {
			t.Fatalf("view lacks %q:\n%s", want, text)
		}
	}
}
