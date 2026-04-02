package fs

import (
	"os"
	"path/filepath"
	"sort"
)

// WalkFiles calls fn for every regular file under root, recursively, in lexical
// order.  Unreadable entries are skipped silently.
func WalkFiles(root string, fn func(path string) error) error {
	return filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil // skip unreadable entries
		}
		if d.IsDir() || d.Type()&os.ModeSymlink != 0 {
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
			continue
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
