package fs

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
)

// skipDirs contains directory names that are never synced.
var skipDirs = map[string]bool{
	".git":         true,
	".svn":         true,
	".hg":          true,
	"node_modules": true,
	".idea":        true,
	".vscode":      true,
}

// ShouldSkipDir reports whether name is a directory that drift should never sync.
func ShouldSkipDir(name string) bool {
	return skipDirs[name]
}

// WalkFiles calls fn for every regular file under root, recursively, in lexical
// order. Directories in skipDirs are skipped, and so is everything that is not
// a regular file: symlinks, FIFOs, sockets and devices. Reading a FIFO would
// block until someone writes to it, and a device is not a project file. Read
// errors stop the walk.
func WalkFiles(root string, fn func(path string) error) error {
	return filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			if path != root && ShouldSkipDir(d.Name()) {
				return filepath.SkipDir
			}
			return nil
		}
		if !d.Type().IsRegular() {
			return nil
		}
		return fn(path)
	})
}

// ReadDir reads one level of a directory.
// Directories are returned before files; both groups sorted alphabetically.
func ReadDir(dir string) ([]*FileEntry, error) {
	des, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}

	var dirs, files []*FileEntry
	for _, de := range des {
		info, err := de.Info()
		if err != nil {
			return nil, fmt.Errorf("stat %s: %w", filepath.Join(dir, de.Name()), err)
		}

		kind := EntryFile
		switch {
		case de.IsDir():
			kind = EntryDir
		case de.Type()&os.ModeSymlink != 0:
			kind = EntrySymlink
		}

		fe := &FileEntry{
			Name:    de.Name(),
			Path:    filepath.Join(dir, de.Name()),
			Kind:    kind,
			Size:    info.Size(),
			ModTime: info.ModTime(),
			Mode:    info.Mode(),
		}

		if kind == EntryDir {
			dirs = append(dirs, fe)
		} else {
			files = append(files, fe)
		}
	}

	sort.Slice(dirs, func(i, j int) bool { return dirs[i].Name < dirs[j].Name })
	sort.Slice(files, func(i, j int) bool { return files[i].Name < files[j].Name })

	return append(dirs, files...), nil
}
