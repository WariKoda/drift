package architecture

import (
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

const module = "github.com/WariKoda/drift/internal/"

func violations(dir, source string) ([]string, error) {
	file, err := parser.ParseFile(token.NewFileSet(), "source.go", source, 0)
	if err != nil {
		return nil, err
	}
	var found []string
	imports := map[string]string{}
	for _, spec := range file.Imports {
		path, err := strconv.Unquote(spec.Path.Value)
		if err != nil {
			return nil, err
		}
		name := filepath.Base(path)
		if spec.Name != nil {
			name = spec.Name.Name
		}
		imports[name] = path
		if dir == "app" || dir == "sync" {
			if path == "github.com/charmbracelet/bubbletea" || path == module+"tui" || strings.HasPrefix(path, module+"tui/") {
				found = append(found, "application logic imports UI: "+path)
			}
		}
		if dir == "tui" || strings.HasPrefix(dir, "tui/") {
			if path == module+"sftp" || path == module+"ftp" || path == "github.com/pkg/sftp" || path == "github.com/jlaffaye/ftp" {
				found = append(found, "UI imports protocol implementation: "+path)
			}
			if name == "." && (path == module+"remote" || path == module+"diff") {
				found = append(found, "UI hides guarded calls with dot import: "+path)
			}
		}
	}
	if dir == "tui" || strings.HasPrefix(dir, "tui/") {
		ast.Inspect(file, func(node ast.Node) bool {
			selector, ok := node.(*ast.SelectorExpr)
			if !ok {
				return true
			}
			ident, ok := selector.X.(*ast.Ident)
			if ok && ((imports[ident.Name] == module+"remote" && selector.Sel.Name == "Connect") ||
				(imports[ident.Name] == module+"diff" && (selector.Sel.Name == "Compare" || selector.Sel.Name == "CompareWithActivity"))) {
				found = append(found, "UI references orchestration function: "+ident.Name+"."+selector.Sel.Name)
			}
			return true
		})
	}
	return found, nil
}

func TestProductionBoundaries(t *testing.T) {
	root := ".."
	err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return nil
		}
		dir, err := filepath.Rel(root, filepath.Dir(path))
		if err != nil {
			return err
		}
		file, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		found, err := violations(filepath.ToSlash(dir), string(file))
		if err != nil {
			return err
		}
		for _, violation := range found {
			t.Errorf("%s: %s", path, violation)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}

func TestBoundaryFixtures(t *testing.T) {
	for _, test := range []struct {
		dir, source string
		invalid     bool
	}{
		{"app", `package app; import tea "github.com/charmbracelet/bubbletea"; var _ tea.Msg`, true},
		{"sync", `package sync; import _ "github.com/WariKoda/drift/internal/tui/browser"`, true},
		{"tui/browser", `package browser; import r "github.com/WariKoda/drift/internal/remote"; var _ = r.Connect`, true},
		{"tui/diffview", `package diffview; import d "github.com/WariKoda/drift/internal/diff"; var _ = d.CompareWithActivity`, true},
		{"tui/browser", `package browser; import _ "github.com/pkg/sftp"`, true},
		{"tui/browser", `package browser; import . "github.com/WariKoda/drift/internal/remote"`, true},
		{"tui/diffview", `package diffview; import d "github.com/WariKoda/drift/internal/diff"; var _ d.Session`, false},
		{"app", `package app; import r "github.com/WariKoda/drift/internal/remote"; var _ = r.Connect`, false},
	} {
		found, err := violations(test.dir, test.source)
		if err != nil {
			t.Fatal(err)
		}
		if (len(found) > 0) != test.invalid {
			t.Errorf("%s: violations = %v, invalid = %t", test.source, found, test.invalid)
		}
	}
}
