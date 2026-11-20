//go:build unix

package termimage

import (
	"os"

	"golang.org/x/sys/unix"
)

// cellSize returns the pixel size of one terminal cell. Terminals that do not
// report pixel dimensions get a common 1:2 cell.
func cellSize(file *os.File) (int, int) {
	size, err := unix.IoctlGetWinsize(int(file.Fd()), unix.TIOCGWINSZ)
	if err != nil || size.Col == 0 || size.Row == 0 || size.Xpixel == 0 || size.Ypixel == 0 {
		return defaultCellWidth, defaultCellHeight
	}
	return max(1, int(size.Xpixel)/int(size.Col)), max(1, int(size.Ypixel)/int(size.Row))
}
