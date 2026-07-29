package hostform

import (
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

func TestMappingEditorKeepsInvalidMappingOpen(t *testing.T) {
	model := New(config.ScopeProject, "/workspace/project", 100, 40)
	model.openMappingEdit(-1)
	model.editFields[0].SetValue("../outside")
	model.editFields[1].SetValue("app")

	updated, _ := model.handleMappingEdit(tea.KeyMsg{Type: tea.KeyCtrlS})

	if updated.sub != subMappingEdit {
		t.Fatalf("mapping editor closed after validation failure, sub = %v", updated.sub)
	}
	if len(updated.mappings) != 0 {
		t.Fatalf("invalid mapping was saved: %#v", updated.mappings)
	}
	if !strings.Contains(updated.errMsg, `must not contain ".." segments`) {
		t.Fatalf("error message = %q, want local containment error", updated.errMsg)
	}
	if !strings.Contains(updated.viewMappingEdit(), updated.errMsg) {
		t.Fatal("mapping editor does not render its validation error")
	}
}

func TestMappingEditorRejectsCollisionWithoutReplacingMappings(t *testing.T) {
	model := New(config.ScopeProject, "/workspace/project", 100, 40)
	model.mappings = []config.Mapping{{Local: "plugins/one", Remote: "remote/one"}}
	model.openMappingEdit(-1)
	model.editFields[0].SetValue("plugins/two")
	model.editFields[1].SetValue("remote/one")

	updated, _ := model.handleMappingEdit(tea.KeyMsg{Type: tea.KeyCtrlS})

	if updated.sub != subMappingEdit {
		t.Fatalf("mapping editor closed after collision, sub = %v", updated.sub)
	}
	if len(updated.mappings) != 1 {
		t.Fatalf("collision changed mappings: %#v", updated.mappings)
	}
	if !strings.Contains(updated.errMsg, "same remote path") {
		t.Fatalf("error message = %q, want remote collision error", updated.errMsg)
	}
}

func TestMappingEditorSavesValidMapping(t *testing.T) {
	model := New(config.ScopeProject, "/workspace/project", 100, 40)
	model.openMappingEdit(-1)
	model.editFields[0].SetValue("plugins/one")
	model.editFields[1].SetValue("custom/plugins/one")

	updated, _ := model.handleMappingEdit(tea.KeyMsg{Type: tea.KeyCtrlS})

	if updated.sub != subMappingList {
		t.Fatalf("mapping editor stayed open after valid save, sub = %v", updated.sub)
	}
	if len(updated.mappings) != 1 {
		t.Fatalf("saved mappings = %#v, want one mapping", updated.mappings)
	}
	if updated.errMsg != "" {
		t.Fatalf("validation error remained after valid save: %q", updated.errMsg)
	}
}

func TestToHostRejectsPreexistingInvalidMappings(t *testing.T) {
	model := NewEdit(config.Host{
		Name:     "prod",
		Hostname: "example.com",
		RootPath: "/var/www",
		Mappings: []config.Mapping{{Local: "app", Remote: "/absolute"}},
	}, config.ScopeProject, "/workspace/project", 100, 40)

	if _, err := model.toHost(); err == nil || !strings.Contains(err.Error(), "Mappings:") {
		t.Fatalf("toHost error = %v, want mappings validation error", err)
	}
}
