package parity

import (
	"bufio"
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/BurntSushi/toml"
	"github.com/WariKoda/drift/internal/config"
)

func TestGoRustServerPromotionTargetsAndConflict(t *testing.T) {
	binary := probe(t)
	var referenceGlobal config.GlobalConfig
	var referenceSource config.ProjectConfig
	for _, engine := range []string{"go", "rust", "conflict"} {
		t.Run(engine, func(t *testing.T) {
			t.Setenv("XDG_CONFIG_HOME", t.TempDir())
			projects := filepath.Join(config.Dir(), "projects")
			if err := os.MkdirAll(projects, 0700); err != nil {
				t.Fatal(err)
			}
			globalPath := filepath.Join(config.Dir(), "config.toml")
			sourcePath := filepath.Join(projects, "source.toml")
			global := "[defaults]\nport=2222\nuser='global'\n[[hosts]]\nname='stage'\nhostname='elsewhere'\n[[hosts]]\nname='source-stage'\nhostname='elsewhere'\n[[hosts]]\nname='source-stage-2'\nhostname='elsewhere'\n"
			source := "[defaults]\nuser='web'\n[[hosts]]\nname='stage'\nhostname='stage.example'\nroot_path='/source'\nkeep_alive_interval=0\n[hosts.auth]\ntype='password'\npassword='test-secret'\n[[hosts.mappings]]\nlocal='src'\nremote='app'\n[[hosts]]\nname='unrelated'\nhostname='other.example'\n"
			if err := os.WriteFile(globalPath, []byte(global), 0600); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(sourcePath, []byte(source), 0600); err != nil {
				t.Fatal(err)
			}
			root := t.TempDir()
			cfg, err := config.Load(root, "dest")
			if err != nil {
				t.Fatal(err)
			}
			targets, err := config.LinkTargets(cfg)
			if err != nil {
				t.Fatal(err)
			}
			var rows strings.Builder
			for _, target := range targets {
				protocol := target.Host.Protocol
				if protocol == "" {
					protocol = "sftp"
				}
				fmt.Fprintf(&rows, "%s|%s|%s|%d|%s|%s|%s\n", target.Project, target.Host.Name, target.Host.Hostname, target.Host.Port, target.Host.User, protocol, strings.Join(target.UsedBy, ","))
			}
			output, err := exec.Command(binary, config.Dir(), "targets", "dest").CombinedOutput()
			if err != nil || string(output) != rows.String() {
				t.Fatalf("link targets differ: %v\n%s", err, output)
			}
			switch engine {
			case "go":
				server, err := config.PromoteProjectHost(cfg, "source", "stage")
				if err != nil || server.Name != "source-stage-3" {
					t.Fatalf("Go promotion: %v", err)
				}
			case "rust":
				lock, err := config.LockWrites()
				if err != nil {
					t.Fatal(err)
				}
				output, promoteErr := exec.Command(binary, config.Dir(), "promote", "source", "stage", "dest").CombinedOutput()
				if err = lock.Close(); err != nil {
					t.Fatal(err)
				}
				if promoteErr == nil || !strings.Contains(string(output), "another drift process") {
					t.Fatalf("Rust promoted through Go lock: %v %s", promoteErr, output)
				}
				output, err = exec.Command(binary, config.Dir(), "promote", "source", "stage", "dest").CombinedOutput()
				if err != nil || strings.TrimSpace(string(output)) != "source-stage-3" {
					t.Fatalf("Rust promotion: %v %s", err, output)
				}
			case "conflict":
				ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
				defer cancel()
				command := exec.CommandContext(ctx, binary, config.Dir(), "promote-paused", "source", "stage", "dest")
				input, err := command.StdinPipe()
				if err != nil {
					t.Fatal(err)
				}
				output, err := command.StdoutPipe()
				if err != nil {
					t.Fatal(err)
				}
				var stderr bytes.Buffer
				command.Stderr = &stderr
				if err = command.Start(); err != nil {
					t.Fatal(err)
				}
				defer input.Close()
				ready, err := bufio.NewReader(output).ReadString('\n')
				if err != nil || ready != "selected\n" {
					t.Fatalf("selection readiness: %q %v", ready, err)
				}
				sourceConfig, err := config.Load(root, "source")
				if err != nil {
					t.Fatal(err)
				}
				edited := sourceConfig.Hosts["stage"]
				edited.Hostname = "changed.example"
				if err = config.SaveProjectHost(sourceConfig, edited, "stage"); err != nil {
					t.Fatal(err)
				}
				if _, err = input.Write([]byte("x")); err != nil {
					t.Fatal(err)
				}
				if err = command.Wait(); err == nil || !strings.Contains(stderr.String(), "changed in another drift process") {
					t.Fatalf("stale Rust promotion overwrote Go edit: %v %s", err, stderr.String())
				}
				actualGlobal, err := os.ReadFile(globalPath)
				if err != nil || string(actualGlobal) != global {
					t.Fatal("stale promotion wrote the global store")
				}
				return
			}
			if _, err := os.Stat(filepath.Join(projects, "dest.toml")); !os.IsNotExist(err) {
				t.Fatalf("promotion saved destination: %v", err)
			}
			var actualGlobal config.GlobalConfig
			var actualSource config.ProjectConfig
			if _, err = toml.DecodeFile(globalPath, &actualGlobal); err != nil {
				t.Fatal(err)
			}
			if _, err = toml.DecodeFile(sourcePath, &actualSource); err != nil {
				t.Fatal(err)
			}
			if engine == "go" {
				referenceGlobal = actualGlobal
				referenceSource = actualSource
			}
			if !reflect.DeepEqual(actualGlobal, referenceGlobal) || !reflect.DeepEqual(actualSource, referenceSource) {
				t.Fatal("Go and Rust promotion produced different stored records")
			}
			if _, err = config.Load(root, "source"); err != nil {
				t.Fatalf("Go cannot resolve promoted source: %v", err)
			}
			cfg, err = config.Load(root, "dest")
			if err != nil {
				t.Fatal(err)
			}
			if err = config.SaveProjectHost(cfg, config.Host{Name: "live", Server: "source-stage-3", RootPath: "/dest"}, ""); err != nil {
				t.Fatal(err)
			}
			if output, err = exec.Command(binary, config.Dir(), "roundtrip", "source").CombinedOutput(); err != nil {
				t.Fatalf("Rust reread of promoted source: %v %s", err, output)
			}
			if output, err = exec.Command(binary, config.Dir(), "roundtrip", "dest").CombinedOutput(); err != nil {
				t.Fatalf("Rust reread of new Go link: %v %s", err, output)
			}
			contents, err := os.ReadFile(filepath.Join(projects, "dest.toml"))
			if err != nil || strings.Contains(string(contents), "hostname") || strings.Contains(string(contents), "password") {
				t.Fatal("destination link gained server credentials")
			}
			entries, err := os.ReadDir(root)
			if err != nil || len(entries) != 0 {
				t.Fatal("management wrote into the project directory")
			}
		})
	}
}
