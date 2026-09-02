package fs

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

// Root is a project-rooted view of the local filesystem. Every path handed to
// its methods is absolute and has to resolve beneath the project root: a
// symlinked path component that leaves the project fails the operation instead
// of quietly redirecting it. os.Root enforces that in the kernel, so there is
// no window between checking a path and using it.
//
// Path translation in internal/pathmap only guarantees that drift's local paths
// look like project paths. Root is what makes them be project paths. Symlinks
// that stay inside the project are followed as usual.
type Root struct {
	base string
	root *os.Root
}

// OpenRoot opens projectRoot for confined access. Callers must Close it.
func OpenRoot(projectRoot string) (*Root, error) {
	base, err := filepath.Abs(projectRoot)
	if err != nil {
		return nil, fmt.Errorf("resolve project root %s: %w", projectRoot, err)
	}
	root, err := os.OpenRoot(base)
	if err != nil {
		return nil, fmt.Errorf("open project root %s: %w", base, err)
	}
	return &Root{base: base, root: root}, nil
}

// Close releases the directory handle. Closing twice is a no-op.
func (r *Root) Close() error {
	if r == nil || r.root == nil {
		return nil
	}
	root := r.root
	r.root = nil
	return root.Close()
}

// Base reports the absolute project root.
func (r *Root) Base() string {
	if r == nil {
		return ""
	}
	return r.base
}

// Open opens a local file for reading.
func (r *Root) Open(absPath string) (*os.File, error) {
	rel, err := r.rel(absPath)
	if err != nil {
		return nil, err
	}
	return r.root.Open(rel)
}

// Stat stats a local file, following symlinks that stay inside the project.
func (r *Root) Stat(absPath string) (os.FileInfo, error) {
	rel, err := r.rel(absPath)
	if err != nil {
		return nil, err
	}
	return r.root.Stat(rel)
}

// ReadFile reads a local file in full.
func (r *Root) ReadFile(absPath string) ([]byte, error) {
	rel, err := r.rel(absPath)
	if err != nil {
		return nil, err
	}
	return r.root.ReadFile(rel)
}

// Remove deletes a local file.
func (r *Root) Remove(absPath string) error {
	rel, err := r.rel(absPath)
	if err != nil {
		return err
	}
	return r.root.Remove(rel)
}

// WriteAtomic replaces absPath with everything src yields, creating parent
// directories as needed. It writes a staging file beside the target and renames
// it into place, so a failed transfer leaves the previous file untouched. An
// existing target keeps its permission bits; a target that is not a regular
// file is refused.
//
// WriteAtomic closes src, and a close error fails the write before the rename:
// for a remote download, closing the stream is what reports whether the
// transfer actually completed.
func (r *Root) WriteAtomic(absPath string, src io.ReadCloser) error {
	rel, relErr := r.rel(absPath)
	if relErr != nil {
		_ = src.Close()
		return relErr
	}

	if dir := filepath.Dir(rel); dir != "." {
		if err := r.root.MkdirAll(dir, 0o755); err != nil {
			_ = src.Close()
			return fmt.Errorf("mkdir %s: %w", filepath.Join(r.base, dir), err)
		}
	}

	mode := os.FileMode(0o666)
	preserveMode := false
	if info, statErr := r.root.Lstat(rel); statErr == nil {
		if !info.Mode().IsRegular() {
			_ = src.Close()
			return fmt.Errorf("local target %s is not a regular file", absPath)
		}
		mode = info.Mode().Perm()
		preserveMode = true
	} else if !errors.Is(statErr, os.ErrNotExist) {
		_ = src.Close()
		return fmt.Errorf("stat local %s: %w", absPath, statErr)
	}

	stageRel, err := stagingSibling(rel)
	if err != nil {
		_ = src.Close()
		return err
	}
	stage, err := r.root.OpenFile(stageRel, os.O_WRONLY|os.O_CREATE|os.O_EXCL, mode)
	if err != nil {
		_ = src.Close()
		return fmt.Errorf("create staged local file for %s: %w", absPath, err)
	}
	committed := false
	defer func() {
		if !committed {
			_ = r.root.Remove(stageRel)
		}
	}()

	// Chmod on the open handle rather than on the path: os.Root.Chmod resolves
	// the name again, and OpenFile's perm is subject to the umask.
	var chmodErr error
	if preserveMode {
		chmodErr = stage.Chmod(mode)
	}

	_, copyErr := io.Copy(stage, src)
	srcCloseErr := src.Close()
	var syncErr error
	if copyErr == nil && srcCloseErr == nil {
		syncErr = stage.Sync()
	}
	stageCloseErr := stage.Close()

	if err := errors.Join(chmodErr, copyErr, srcCloseErr, syncErr, stageCloseErr); err != nil {
		return fmt.Errorf("write local %s: %w", absPath, err)
	}
	if err := r.root.Rename(stageRel, rel); err != nil {
		return fmt.Errorf("replace local %s: %w", absPath, err)
	}
	committed = true
	return nil
}

// rel turns an absolute local path into a root-relative one, rejecting anything
// that lexically leaves the project. os.Root rejects the rest, including paths
// that only leave it through a symlink.
func (r *Root) rel(absPath string) (string, error) {
	clean := filepath.Clean(absPath)
	if !filepath.IsAbs(clean) {
		return "", fmt.Errorf("local path %s is not absolute", absPath)
	}
	rel, err := filepath.Rel(r.base, clean)
	if err != nil {
		return "", fmt.Errorf("local path %s is outside project %s", absPath, r.base)
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("local path %s is outside project %s", absPath, r.base)
	}
	return rel, nil
}

// stagingSibling returns an unpredictable hidden name beside rel. Staging files
// live next to their target so the final rename stays within one directory.
func stagingSibling(rel string) (string, error) {
	var token [16]byte
	if _, err := rand.Read(token[:]); err != nil {
		return "", fmt.Errorf("generate staging name: %w", err)
	}
	name := "." + filepath.Base(rel) + ".drift-tmp-" + hex.EncodeToString(token[:])
	if dir := filepath.Dir(rel); dir != "." {
		return filepath.Join(dir, name), nil
	}
	return name, nil
}
