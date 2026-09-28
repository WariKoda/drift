package app

import (
	"path"
	"sort"
	"strings"

	"github.com/WariKoda/drift/internal/fs"
)

func sortedMarkedPaths(sel *fs.SelectionState) []string {
	if sel == nil {
		return nil
	}
	paths := make([]string, 0, len(sel.Marked))
	for p := range sel.Marked {
		paths = append(paths, p)
	}
	sort.Strings(paths)
	return paths
}

func remotePathHidden(remotePath, rootPath string) bool {
	root := path.Clean(rootPath)
	if rootPath == "" {
		root = "/"
	}
	candidate := path.Clean(remotePath)
	if candidate != root && root != "/" && !strings.HasPrefix(candidate, root+"/") {
		return false
	}
	relative := strings.TrimPrefix(strings.TrimPrefix(candidate, root), "/")
	for _, part := range strings.Split(relative, "/") {
		if strings.HasPrefix(part, ".") && part != "." && part != ".." {
			return true
		}
	}
	return false
}
