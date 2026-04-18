package pathmap

import (
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/config"
)

func TestLocalToRemote_ProjectMappingsAreEnforced(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, []config.Mapping{{Local: "plugins/plugin1", Remote: "html/custom/plugins/plugin1"}}, config.Host{
		RootPath: "/var/www",
	})

	remote, err := mapper.LocalToRemote(filepath.Join(root, "plugins", "plugin1", "src", "Main.php"))
	if err != nil {
		t.Fatalf("LocalToRemote returned error for mapped file: %v", err)
	}
	if want := "/var/www/html/custom/plugins/plugin1/src/Main.php"; remote != want {
		t.Fatalf("LocalToRemote = %q, want %q", remote, want)
	}

	if _, err := mapper.LocalToRemote(filepath.Join(root, "plugins", "plugin2", "src", "Other.php")); err == nil {
		t.Fatal("LocalToRemote unexpectedly allowed file outside configured project mappings")
	}
}

func TestRemoteToLocal_ProjectMappingsAreEnforced(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, []config.Mapping{{Local: "plugins/plugin1", Remote: "html/custom/plugins/plugin1"}}, config.Host{
		RootPath: "/var/www",
	})

	local, err := mapper.RemoteToLocal("/var/www/html/custom/plugins/plugin1/src/Main.php")
	if err != nil {
		t.Fatalf("RemoteToLocal returned error for mapped file: %v", err)
	}
	if want := filepath.Join(root, "plugins", "plugin1", "src", "Main.php"); local != want {
		t.Fatalf("RemoteToLocal = %q, want %q", local, want)
	}

	if _, err := mapper.RemoteToLocal("/var/www/html/custom/plugins/plugin2/src/Other.php"); err == nil {
		t.Fatal("RemoteToLocal unexpectedly allowed file outside configured project mappings")
	}
}
