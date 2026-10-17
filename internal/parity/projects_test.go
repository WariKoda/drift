package parity

import (
	"bufio"
	"bytes"
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
)

func TestRustProjectArchiveRemovalAndGoEditConflicts(t *testing.T) {
	binary := probe(t)
	for _, operation := range []string{"archive-project", "delete-project", "edit-project-paused", "delete-project-paused"} {
		t.Run(operation, func(t *testing.T) {
			t.Setenv("XDG_CONFIG_HOME", t.TempDir())
			root := t.TempDir()
			local := filepath.Join(root, "keep")
			if err := os.WriteFile(local, []byte("local data"), 0600); err != nil {
				t.Fatal(err)
			}
			store := project.NewStore()
			date := time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC)
			reg := &project.Registry{Projects: []project.Project{
				{Slug: "shop", Name: "Shop", Path: root, CreatedAt: date, UpdatedAt: date},
				{Slug: "other", Name: "Other", Path: filepath.Join(root, "other"), CreatedAt: date, UpdatedAt: date},
			}}
			if err := store.Save(reg); err != nil {
				t.Fatal(err)
			}
			cfg, err := config.Load(root, "shop")
			if err != nil {
				t.Fatal(err)
			}
			if err := config.SaveProjectHost(cfg, config.Host{Name: "prod", Hostname: "prod.example"}, ""); err != nil {
				t.Fatal(err)
			}
			settings := filepath.Join(config.Dir(), "projects/shop.toml")
			before, err := os.ReadFile(settings)
			if err != nil {
				t.Fatal(err)
			}
			if strings.HasSuffix(operation, "-paused") {
				ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
				defer cancel()
				command := exec.CommandContext(ctx, binary, config.Dir(), operation)
				input, err := command.StdinPipe()
				if err != nil {
					t.Fatal(err)
				}
				defer input.Close()
				output, err := command.StdoutPipe()
				if err != nil {
					t.Fatal(err)
				}
				var stderr bytes.Buffer
				command.Stderr = &stderr
				if err := command.Start(); err != nil {
					t.Fatal(err)
				}
				ready, err := bufio.NewReader(output).ReadString('\n')
				if err != nil || ready != "selected\n" {
					t.Fatalf("selection: %v %q", err, ready)
				}
				fresh, err := store.Load()
				if err != nil {
					t.Fatal(err)
				}
				updated := *fresh.Find("shop")
				updated.Name = "Go edit"
				updated.UpdatedAt = time.Now().UTC()
				if err := fresh.Update("shop", updated); err != nil {
					t.Fatal(err)
				}
				if err := store.Save(fresh); err != nil {
					t.Fatal(err)
				}
				// Independent new rows must also survive the stale Rust command.
				fresh, err = store.Load()
				if err != nil {
					t.Fatal(err)
				}
				if err := fresh.Add(project.Project{Slug: "new", Name: "New", Path: filepath.Join(root, "new"), CreatedAt: date, UpdatedAt: date}); err != nil {
					t.Fatal(err)
				}
				if err := store.Save(fresh); err != nil {
					t.Fatal(err)
				}
				if _, err := input.Write([]byte("x")); err != nil {
					t.Fatal(err)
				}
				if err := command.Wait(); err == nil || !strings.Contains(stderr.String(), "changed in another drift process") {
					t.Fatalf("stale command: %v %s", err, stderr.String())
				}
				fresh, err = store.Load()
				if err != nil {
					t.Fatal(err)
				}
				if fresh.Find("shop").Name != "Go edit" || fresh.Find("new") == nil {
					t.Fatal("lost Go registry changes")
				}
			} else {
				lock, err := config.LockWrites()
				if err != nil {
					t.Fatal(err)
				}
				output, commandErr := exec.Command(binary, config.Dir(), operation).CombinedOutput()
				if err := lock.Close(); err != nil {
					t.Fatal(err)
				}
				if commandErr == nil || !strings.Contains(string(output), "another drift process") {
					t.Fatalf("Rust ignored Go lock: %v %s", commandErr, output)
				}
				if output, err := exec.Command(binary, config.Dir(), operation).CombinedOutput(); err != nil {
					t.Fatalf("Rust mutation: %v %s", err, output)
				}
				fresh, err := store.Load()
				if err != nil {
					t.Fatal(err)
				}
				if fresh.Find("other") == nil {
					t.Fatal("lost independent project")
				}
				if operation == "archive-project" {
					after, err := os.ReadFile(settings)
					if err != nil || !bytes.Equal(before, after) {
						t.Fatal("archive changed settings")
					}
					archived := fresh.Find("shop")
					if archived == nil || !archived.Archived || !archived.CreatedAt.Equal(date) {
						t.Fatal("invalid archived project")
					}
					archived.Archived = false
					archived.UpdatedAt = time.Now().UTC()
					if err := store.Save(fresh); err != nil {
						t.Fatal(err)
					}
					if output, err := exec.Command(binary, config.Dir(), "roundtrip").CombinedOutput(); err != nil {
						t.Fatalf("unarchive roundtrip: %v %s", err, output)
					}
				} else {
					if fresh.Find("shop") != nil {
						t.Fatal("project not removed")
					}
					if _, err := os.Stat(settings); !os.IsNotExist(err) {
						t.Fatalf("settings not removed: %v", err)
					}
				}
			}
			if strings.HasSuffix(operation, "-paused") {
				after, err := os.ReadFile(settings)
				if err != nil || !bytes.Equal(before, after) {
					t.Fatal("settings changed")
				}
			}
			if contents, err := os.ReadFile(local); err != nil || string(contents) != "local data" {
				t.Fatal("local project content changed")
			}
		})
	}
}
