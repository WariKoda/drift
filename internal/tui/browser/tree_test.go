package browser

import (
	"os"
	"path/filepath"
	"testing"
)

func TestExpandAtRevealsAllChildrenWhenTheyFit(t *testing.T) {
	model := localExpansionTestModel(t, 3)

	if err := model.expandAt(model.cursor); err != nil {
		t.Fatalf("expand directory: %v", err)
	}

	if model.offset != 3 {
		t.Fatalf("offset = %d, want 3 so the directory and all children are visible", model.offset)
	}
}

func TestExpandAtPinsDirectoryToTopWhenChildrenDoNotFit(t *testing.T) {
	model := localExpansionTestModel(t, 6)

	if err := model.expandAt(model.cursor); err != nil {
		t.Fatalf("expand directory: %v", err)
	}

	if model.offset != model.cursor {
		t.Fatalf("offset = %d, want directory index %d at the top", model.offset, model.cursor)
	}
}

func localExpansionTestModel(t *testing.T, childCount int) Model {
	t.Helper()
	root := t.TempDir()
	for _, name := range []string{"a", "b", "c", "d", "target"} {
		if err := os.Mkdir(filepath.Join(root, name), 0o755); err != nil {
			t.Fatalf("create directory %s: %v", name, err)
		}
	}
	for i := range childCount {
		name := string(rune('a'+i)) + ".txt"
		if err := os.WriteFile(filepath.Join(root, "target", name), []byte(name), 0o644); err != nil {
			t.Fatalf("create child %s: %v", name, err)
		}
	}

	model, err := New(root)
	if err != nil {
		t.Fatalf("create browser model: %v", err)
	}
	model.Height = 11 // five entry rows
	model.cursor = 4
	return model
}
