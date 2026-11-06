package remoteedit

import (
	"errors"
	"os/exec"
	"path/filepath"

	"github.com/WariKoda/drift/internal/config"
)

// Command never interprets a shell expression or a remote path.
func Command(editor *config.Editor, file string) (*exec.Cmd, error) {
	if err := editor.Validate(); err != nil {
		return nil, err
	}
	if !filepath.IsAbs(file) {
		return nil, errors.New("editor working file must be absolute")
	}
	args := append([]string(nil), editor.Command...)
	for i := 1; i < len(args); i++ {
		if args[i] == "{file}" {
			args[i] = file
		}
	}
	command := exec.Command(args[0], args[1:]...)
	command.Dir = filepath.Dir(file)
	return command, nil
}
