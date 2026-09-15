package tui

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/projectselector"
)

func appWithMalformedRegistry(t *testing.T, screen Screen) (App, *project.Registry) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store := project.NewStore()
	if err := os.MkdirAll(filepath.Dir(store.Path()), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(store.Path(), []byte("[[projects]\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	reg := &project.Registry{Projects: []project.Project{{Slug: "old", Path: "/work/old"}}}
	return App{store: store, registry: reg, state: AppState{Screen: screen}}, reg
}

func TestRegistryReloadFailureKeepsCurrentScreenAndSnapshot(t *testing.T) {
	t.Run("browser picker", func(t *testing.T) {
		a, reg := appWithMalformedRegistry(t, ScreenBrowser)
		updated, _ := a.Update(browser.MsgOpenDashboard{})
		got := updated.(App)
		if got.state.Screen != ScreenBrowser {
			t.Fatalf("screen = %v, want browser", got.state.Screen)
		}
		if got.registry != reg {
			t.Fatal("failed reload replaced the registry snapshot")
		}
	})

	t.Run("selector dashboard", func(t *testing.T) {
		a, reg := appWithMalformedRegistry(t, ScreenProjectSelector)
		updated, _ := a.Update(projectselector.MsgOpenDashboard{})
		got := updated.(App)
		if got.state.Screen != ScreenProjectSelector {
			t.Fatalf("screen = %v, want project selector", got.state.Screen)
		}
		if got.registry != reg {
			t.Fatal("failed reload replaced the registry snapshot")
		}
	})
}

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
