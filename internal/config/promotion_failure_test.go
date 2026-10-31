package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"golang.org/x/sys/unix"
)

func TestPromoteProjectHostRollsBackFailedProjectWrite(t *testing.T) {
	for _, existingGlobal := range []bool{false, true} {
		t.Run(fmt.Sprintf("existing-global-%t", existingGlobal), func(t *testing.T) {
			isolate(t)
			original := GlobalConfig{}
			wantName := "staging"
			if existingGlobal {
				original = GlobalConfig{
					Defaults: Defaults{Port: 2222, User: "global"},
					Hosts:    []Host{{Name: "staging", Hostname: "elsewhere"}},
				}
				if err := writeGlobal(original); err != nil {
					t.Fatal(err)
				}
				wantName = "shop-a-staging"
			}
			writeStore(t, "shop-b", "[[hosts]]\nname = 'own'\nhostname = 'own.example'\n")
			root := t.TempDir()
			cfg, err := Load(root, "shop-b")
			if err != nil {
				t.Fatal(err)
			}
			before, err := Load(root, "shop-b")
			if err != nil {
				t.Fatal(err)
			}
			path, err := projectStorePath("shop-a")
			if err != nil {
				t.Fatal(err)
			}
			const source = `
[defaults]
user = "web"
[[hosts]]
name = "staging"
hostname = "staging.example"
root_path = "/stage"
[hosts.auth]
type = "password"
password = "$STAGING_PASSWORD"
`
			if err := unix.Mkfifo(path, 0o600); err != nil {
				t.Fatal(err)
			}
			// Opening the FIFO synchronizes with the real store reader. Replace
			// its path before delivering TOML, so the later atomic rename fails
			// against a directory, even when the tests run as root.
			writerDone := make(chan error, 1)
			go func() {
				writerDone <- func() (result error) {
					stream, err := os.OpenFile(path, os.O_WRONLY, 0)
					if err != nil {
						return err
					}
					defer func() { result = errors.Join(result, stream.Close()) }()
					lock, err := LockWrites()
					if !errors.Is(err, ErrWriteBusy) {
						return errors.Join(fmt.Errorf("promotion did not hold write lock: %v", err), lock.Close())
					}
					if err := os.Remove(path); err != nil {
						return err
					}
					if err := os.Mkdir(path, 0o700); err != nil {
						return err
					}
					_, err = stream.WriteString(source)
					return err
				}()
			}()

			server, promoteErr := PromoteProjectHost(cfg, "shop-a", "staging")
			select {
			case err := <-writerDone:
				if err != nil {
					t.Fatal(err)
				}
			case <-time.After(5 * time.Second):
				t.Fatal("project store reader never opened the FIFO")
			}
			var renameErr *os.LinkError
			if !errors.As(promoteErr, &renameErr) || renameErr.New != path {
				t.Fatalf("promotion error = %v, want the project rename failure", promoteErr)
			}
			if !reflect.DeepEqual(server, Host{}) {
				t.Fatalf("failed promotion returned a server: %+v", server)
			}
			if !reflect.DeepEqual(cfg, before) {
				t.Fatalf("failed promotion changed cfg:\ngot  %+v\nwant %+v", cfg, before)
			}
			if existingGlobal {
				stored, err := loadGlobal()
				if err != nil || !reflect.DeepEqual(*stored, original) {
					t.Fatalf("global rollback = %+v, err = %v, want %+v", stored, err, original)
				}
			} else if _, err := os.Stat(globalConfigPath()); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("rollback left a new global config: %v", err)
			}

			if err := os.Remove(path); err != nil {
				t.Fatal(err)
			}
			writeStore(t, "shop-a", source)
			server, err = PromoteProjectHost(cfg, "shop-a", "staging")
			if err != nil || server.Name != wantName {
				t.Fatalf("retry returned %+v, err = %v, want server %q", server, err, wantName)
			}
			stored, err := loadGlobal()
			if err != nil || len(stored.Hosts) != len(original.Hosts)+1 || len(cfg.GlobalHosts) != len(stored.Hosts) {
				t.Fatalf("retry duplicated a server: global = %+v, cfg = %+v, err = %v", stored, cfg.GlobalHosts, err)
			}
			project, err := loadProjectStore("shop-a")
			if err != nil || project == nil || len(project.Hosts) != 1 || project.Hosts[0].Server != wantName {
				t.Fatalf("retry did not write the source link: %+v, err = %v", project, err)
			}
			for _, dir := range []string{Dir(), projectsDir()} {
				staging, err := filepath.Glob(filepath.Join(dir, ".*.toml.*"))
				if err != nil || len(staging) != 0 {
					t.Fatalf("temporary config files remain: %v, err = %v", staging, err)
				}
			}
		})
	}
}
