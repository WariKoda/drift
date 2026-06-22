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

func TestLocalToRemote_DoesNotMatchSiblingPrefix(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, []config.Mapping{{Local: "plugins/foo", Remote: "html/custom/plugins/foo"}}, config.Host{
		RootPath: "/var/www",
	})

	if _, err := mapper.LocalToRemote(filepath.Join(root, "plugins", "foobar", "src", "Main.php")); err == nil {
		t.Fatal("LocalToRemote unexpectedly matched sibling path with shared prefix")
	}
}

func TestRemoteToLocal_DoesNotMatchSiblingPrefix(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, []config.Mapping{{Local: "plugins/foo", Remote: "html/custom/plugins/foo"}}, config.Host{
		RootPath: "/var/www",
	})

	if _, err := mapper.RemoteToLocal("/var/www/html/custom/plugins/foobar/src/Main.php"); err == nil {
		t.Fatal("RemoteToLocal unexpectedly matched sibling path with shared prefix")
	}
}

func TestRemoteToLocal_FallbackRequiresSegmentSafeHostRoot(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, nil, config.Host{RootPath: "/var/www/app"})

	if _, err := mapper.RemoteToLocal("/var/www/application/index.php"); err == nil {
		t.Fatal("RemoteToLocal unexpectedly allowed path outside host root with shared prefix")
	}
}

func TestRemoteToLocal_RootIsSlash(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, nil, config.Host{RootPath: "/"})

	local, err := mapper.RemoteToLocal("/winarbor_exchange/file.txt")
	if err != nil {
		t.Fatalf("RemoteToLocal returned error for host root %q: %v", "/", err)
	}
	if want := filepath.Join(root, "winarbor_exchange", "file.txt"); local != want {
		t.Fatalf("RemoteToLocal = %q, want %q", local, want)
	}
}

func TestLocalToRemote_RootIsSlash(t *testing.T) {
	root := filepath.Join(string(filepath.Separator), "workspace", "project")
	mapper := New(root, nil, config.Host{RootPath: "/"})

	remote, err := mapper.LocalToRemote(filepath.Join(root, "winarbor_exchange", "file.txt"))
	if err != nil {
		t.Fatalf("LocalToRemote returned error for host root %q: %v", "/", err)
	}
	if want := "/winarbor_exchange/file.txt"; remote != want {
		t.Fatalf("LocalToRemote = %q, want %q", remote, want)
	}

	// The project root itself maps back to the host root, not an empty string.
	rootRemote, err := mapper.LocalToRemote(root)
	if err != nil {
		t.Fatalf("LocalToRemote returned error for project root: %v", err)
	}
	if rootRemote != "/" {
		t.Fatalf("LocalToRemote(root) = %q, want %q", rootRemote, "/")
	}
}
