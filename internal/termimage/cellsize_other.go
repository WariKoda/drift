//go:build !unix

package termimage

import "os"

func cellSize(*os.File) (int, int) { return defaultCellWidth, defaultCellHeight }
