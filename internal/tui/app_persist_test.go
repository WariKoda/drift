package tui

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/project"
)

func TestFailedPersistLeavesRegistryUnchanged(t *testing.T) {
	blocker := filepath.Join(t.TempDir(), "not-a-directory")
	if err := os.WriteFile(blocker, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(blocker, "config"))

	reg := &project.Registry{Projects: []project.Project{{
		Slug: "prod", Name: "Production", Path: "/work/prod",
	}}}
	a := App{store: project.NewStore(), registry: reg}

	err := a.persist(func(candidate *project.Registry) error {
		return candidate.Add(project.Project{Slug: "staging", Name: "Staging", Path: "/work/staging"})
	})
	if err == nil {
		t.Fatal("persist returned no error although the registry could not be written")
	}
	if a.registry != reg {
		t.Fatal("persist replaced the live registry after a failed write")
	}
	if len(reg.Projects) != 1 || reg.Find("staging") != nil {
		t.Fatalf("failed mutation leaked into the live registry: %+v", reg.Projects)
	}
}
