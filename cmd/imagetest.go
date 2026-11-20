package cmd

import (
	"fmt"
	"image"
	"image/color"
	"io"
	"os"

	"github.com/WariKoda/drift/internal/imagefile"
	"github.com/WariKoda/drift/internal/termimage"
	"github.com/charmbracelet/x/term"
	"github.com/spf13/cobra"
)

var imageTestCmd = &cobra.Command{
	Use:   "image-test [image]",
	Short: "Check whether this terminal shows images in previews and diffs",
	Long: `image-test prints an image the way drift's preview and diff view draw it:
with the Kitty graphics protocol and Unicode placeholders. Without an argument
it draws a test pattern.

Kitty and Ghostty are detected automatically. DRIFT_IMAGES=off turns images
off; DRIFT_IMAGES=kitty forces them on, for example inside tmux with
allow-passthrough enabled.`,
	Args: cobra.MaximumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		out := cmd.OutOrStdout()
		detection := termimage.Detect(os.Getenv)
		fmt.Fprintf(out, "Images: %s (%s)\n", onOff(detection.Enabled), detection.Reason)
		if !detection.Enabled {
			return nil
		}

		img, label, err := imageTestSource(args)
		if err != nil {
			return err
		}
		cols, rows := 60, 15
		if width, height, err := term.GetSize(os.Stdout.Fd()); err == nil {
			cols, rows = max(1, width-2), max(1, min(rows, height-4))
		}
		output := termimage.NewOutput(os.Stdout, detection)
		placement, err := output.Show(img, cols, rows)
		if err != nil {
			return err
		}
		fmt.Fprintln(out, label)
		for _, row := range termimage.Rows(placement) {
			fmt.Fprintln(out, row)
		}
		fmt.Fprintf(out, "%d × %d cells. Text characters instead of the image mean the terminal lacks Unicode placeholders.\n", placement.Cols, placement.Rows)
		return nil
	},
}

func init() {
	rootCmd.AddCommand(imageTestCmd)
}

func onOff(enabled bool) string {
	if enabled {
		return "on"
	}
	return "off"
}

func imageTestSource(args []string) (image.Image, string, error) {
	if len(args) == 0 {
		return testPattern(), "Test pattern 320 × 160", nil
	}
	file, err := os.Open(args[0])
	if err != nil {
		return nil, "", err
	}
	defer file.Close()
	data, err := io.ReadAll(io.LimitReader(file, imagefile.MaxBytes+1))
	if err != nil {
		return nil, "", err
	}
	if len(data) > imagefile.MaxBytes {
		return nil, "", fmt.Errorf("%s exceeds %d MiB", args[0], imagefile.MaxBytes>>20)
	}
	img, info, err := imagefile.Decode(data, 2048)
	if err != nil {
		return nil, "", fmt.Errorf("%s: %w", args[0], err)
	}
	return img, info.String(), nil
}

// testPattern has colour gradients, a checkerboard and a transparent corner,
// so wrong colours, scaling or alpha handling are visible at a glance.
func testPattern() image.Image {
	const width, height = 320, 160
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for y := range height {
		for x := range width {
			c := color.NRGBA{R: uint8(x * 255 / width), G: uint8(y * 255 / height), B: 160, A: 255}
			if y >= height/2 && (x/16+y/16)%2 == 0 {
				c = color.NRGBA{R: 240, G: 240, B: 240, A: 255}
			}
			if x >= width*3/4 && y < height/2 {
				c.A = uint8((width - x) * 255 / (width / 4))
			}
			img.SetNRGBA(x, y, c)
		}
	}
	return img
}
