package config

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestEditorValidation(t *testing.T) {
	for _, command := range [][]string{
		nil, {"nano"}, {"", "{file}"}, {"nano", "none"},
		{"nano", "{file}", "{file}"}, {"{file}", "{file}"},
		{"code", "--file={file}"}, {"nano", "{unknown}"}, {"nano\x00", "{file}"},
	} {
		if err := (&Editor{Command: command}).Validate(); err == nil {
			t.Fatalf("invalid command accepted: %q", command)
		}
	}
	for _, command := range [][]string{{"nano", "{file}"}, {"zed", "--wait", "{file}"}} {
		editor := &Editor{Command: command}
		if err := editor.Validate(); err != nil || !editor.UsesTerminal() {
			t.Fatalf("valid editor rejected: %v", err)
		}
	}
	var missing *Editor
	if missing.Validate() == nil {
		t.Fatal("missing configuration accepted")
	}
}

func TestEditorSurvivesHostWrites(t *testing.T) {
	for _, existing := range []bool{false, true} {
		t.Run(map[bool]string{false: "new file", true: "existing file"}[existing], func(t *testing.T) {
			isolate(t)
			root := t.TempDir()
			terminal := false
			editor := &Editor{Command: []string{"zed", "--wait", "{file}"}, Terminal: &terminal}
			if existing {
				if err := os.MkdirAll(Dir(), 0700); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(Dir(), "config.toml"), []byte("[editor]\ncommand = ['zed', '--wait', '{file}']\nterminal = false\n"), 0600); err != nil {
					t.Fatal(err)
				}
			}
			cfg, err := Load(root, "")
			if err != nil {
				t.Fatal(err)
			}
			cfg.Editor = editor
			if err := SaveGlobalHost(cfg, Host{Name: "host", Hostname: "example.test"}, ""); err != nil {
				t.Fatal(err)
			}
			cfg, err = Load(root, "")
			if err != nil || !reflect.DeepEqual(cfg.Editor, editor) {
				t.Fatalf("editor lost on save: %+v, %v", cfg, err)
			}
			if err := DeleteGlobalHost(cfg, "host"); err != nil {
				t.Fatal(err)
			}
			cfg, err = Load(root, "")
			if err != nil || !reflect.DeepEqual(cfg.Editor, editor) {
				t.Fatalf("editor lost on delete: %+v, %v", cfg, err)
			}
		})
	}
}

func TestProjectCannotConfigureEditor(t *testing.T) {
	isolate(t)
	if err := os.MkdirAll(filepath.Join(Dir(), "projects"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(Dir(), "projects", "test.toml"), []byte("[editor]\ncommand = ['untrusted', '{file}']\n"), 0600); err != nil {
		t.Fatal(err)
	}
	cfg, err := Load(t.TempDir(), "test")
	if err != nil || cfg.Editor != nil {
		t.Fatalf("project supplied an executable: %+v, %v", cfg, err)
	}
}
