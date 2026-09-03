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

// rowOrder is the visible field order, for asserting the grouping.
func rowOrder(m Model) []int { return m.visibleRows() }

func TestFieldOrderFollowsTheStorageLayers(t *testing.T) {
	m := New(config.ScopeProject, "/workspace/project", 100, 40)
	rows := rowOrder(m)

	want := []int{fName, fHostname, fPort, fProtocol, fRootPath, fMappings, fUser, fAuthType, fKeyFile, fPassphrase, fScope}
	if len(rows) != len(want) {
		t.Fatalf("visibleRows() = %v, want %v", rows, want)
	}
	for i := range want {
		if rows[i] != want[i] {
			t.Fatalf("visibleRows() = %v, want %v", rows, want)
		}
	}
	// Enter on the last row saves, so the scope toggle has to stay last.
	if rows[len(rows)-1] != fScope {
		t.Fatal("the scope toggle is no longer the last row")
	}
}

func TestSectionHeadersOnlyForAProjectHost(t *testing.T) {
	project := New(config.ScopeProject, "/workspace/project", 100, 40).View()
	if !strings.Contains(project, "SHARED WITH THE TEAM") || !strings.Contains(project, ".drift/config.toml") {
		t.Fatalf("the project form does not name the shared layer:\n%s", project)
	}
	if !strings.Contains(project, "ONLY ON THIS MACHINE") || !strings.Contains(project, "access.toml") {
		t.Fatalf("the project form does not name the local layer:\n%s", project)
	}

	global := New(config.ScopeGlobal, "/workspace/project", 100, 40).View()
	for _, unwanted := range []string{"SHARED WITH THE TEAM", "ONLY ON THIS MACHINE"} {
		if strings.Contains(global, unwanted) {
			t.Fatalf("a global host has one file, but the form shows %q:\n%s", unwanted, global)
		}
	}
}

func TestSectionHeaderSitsAboveTheFirstFieldOfItsGroup(t *testing.T) {
	m := New(config.ScopeProject, "/workspace/project", 100, 40)

	if _, ok := m.sectionHeader(fHostname); !ok {
		t.Fatal("no header above the first shared field")
	}
	if _, ok := m.sectionHeader(fUser); !ok {
		t.Fatal("no header above the first access field")
	}
	for _, row := range []int{fName, fPort, fProtocol, fRootPath, fMappings, fAuthType, fKeyFile, fScope} {
		if _, ok := m.sectionHeader(row); ok {
			t.Fatalf("row %d starts a group it should not", row)
		}
	}
}
