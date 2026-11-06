package remoteedit

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/WariKoda/drift/internal/config"
)

func TestCommandPassesLiteralFilenameAndPreservesConfiguredArguments(t *testing.T) {
	file := filepath.Join(t.TempDir(), "-file with ü;$(touch SHOULD_NOT_EXIST).txt")
	editor := &config.Editor{Command: []string{os.Args[0], "-test.run=TestEditorProcessHelper", "--", "{file}"}}
	original := append([]string(nil), editor.Command...)
	command, err := Command(editor, file)
	if err != nil {
		t.Fatal(err)
	}
	if command.Dir != filepath.Dir(file) || command.Args[len(command.Args)-1] != file || !reflect.DeepEqual(editor.Command, original) {
		t.Fatalf("command changed arguments: %+v", command)
	}
	command.Env = append(os.Environ(), "DRIFT_TEST_EDITOR_PROCESS=1")
	if output, err := command.CombinedOutput(); err != nil {
		t.Fatalf("real editor helper: %s, %v", output, err)
	}
	if data, err := os.ReadFile(file); err != nil || string(data) != "edited" {
		t.Fatalf("helper did not edit literal path: %q, %v", data, err)
	}
	if _, err := os.Stat(filepath.Join(command.Dir, "SHOULD_NOT_EXIST")); !os.IsNotExist(err) {
		t.Fatal("filename was executed as shell input")
	}
}

func TestEditorProcessHelper(t *testing.T) {
	if os.Getenv("DRIFT_TEST_EDITOR_PROCESS") != "1" {
		return
	}
	if err := os.WriteFile(os.Args[len(os.Args)-1], []byte("edited"), 0600); err != nil {
		os.Exit(2)
	}
	os.Exit(0)
}
