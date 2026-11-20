package tui

import (
	"bytes"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/termimage"
	"github.com/WariKoda/drift/internal/tui/projectselector"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/ansi/kitty"
)

// pump feeds msg to app and keeps feeding the messages its commands produce,
// including batches, until nothing is left or done reports true.
func pump(t *testing.T, app App, msg tea.Msg, done func(App) bool) App {
	t.Helper()
	queue := []tea.Msg{msg}
	deadline := time.Now().Add(5 * time.Second)
	for len(queue) > 0 && !done(app) {
		if time.Now().After(deadline) {
			t.Fatal("messages kept coming")
		}
		next := queue[0]
		queue = queue[1:]
		if batch, ok := next.(tea.BatchMsg); ok {
			for _, cmd := range batch {
				if cmd != nil {
					queue = append(queue, cmd())
				}
			}
			continue
		}
		if next == nil {
			continue
		}
		model, cmd := app.Update(next)
		app = model.(App)
		if cmd != nil {
			queue = append(queue, cmd())
		}
	}
	return app
}

func TestProjectSwitchFreesThePreviewImage(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	oldDir, newDir := t.TempDir(), t.TempDir()
	img := image.NewNRGBA(image.Rect(0, 0, 40, 20))
	for index := range img.Pix {
		img.Pix[index] = 200
	}
	img.SetNRGBA(0, 0, color.NRGBA{R: 1, A: 255})
	var encoded bytes.Buffer
	if err := png.Encode(&encoded, img); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(oldDir, "logo.png"), encoded.Bytes(), 0o600); err != nil {
		t.Fatal(err)
	}
	file, err := os.CreateTemp(t.TempDir(), "terminal")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { file.Close() })
	terminal := func() string {
		data, err := os.ReadFile(file.Name())
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}
	output := termimage.NewOutput(file, termimage.Detection{Mode: termimage.Kitty})

	a, err := New(oldDir, nil, nil, nil, ScreenBrowser, false, output)
	if err != nil {
		t.Fatal(err)
	}
	a = pump(t, a, tea.WindowSizeMsg{Width: 100, Height: 30}, func(App) bool { return false })
	a = pump(t, a, a.Init(), func(App) bool { return false })
	placed := regexp.MustCompile(`a=p,U=1,i=(\d+)`)
	a = pump(t, a, key('p'), func(app App) bool { return strings.ContainsRune(app.View(), kitty.Placeholder) })
	match := placed.FindStringSubmatch(terminal())
	if match == nil {
		t.Fatalf("preview did not place the image; screen:\n%s", ansi.Strip(a.View()))
	}
	var id uint32
	fmt.Sscan(match[1], &id)
	freed := ansi.KittyGraphics(nil, "a=d", "d=I", fmt.Sprintf("i=%d", id), "q=2")

	next := project.Project{Slug: "next", Name: "Next", Path: newDir}
	updated, prepare := a.Update(projectselector.MsgProjectChosen{Project: next})
	a = updated.(App)
	a = pump(t, a, prepare(), func(App) bool { return strings.Contains(terminal(), freed) })
	if a.state.WorkingDir != newDir {
		t.Fatal("project did not switch")
	}
	if !strings.Contains(terminal(), freed) {
		t.Fatal("switching the project kept the preview image in the terminal")
	}
}
