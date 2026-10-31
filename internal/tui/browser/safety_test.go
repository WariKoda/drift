package browser

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/styles"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestHelpCapturesKeyboardWithoutChangingBrowser(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "file.txt")
	writeVisibilityFile(t, path, "content")
	for _, key := range []string{" ", "s", "S", "r", "f", "H", "P", "@", "/", ".", "I", "tab", "down", "esc", "q", "?"} {
		t.Run(key, func(t *testing.T) {
			m, err := New(root)
			if err != nil {
				t.Fatal(err)
			}
			m.Selection.Toggle(path)
			m.showHelp = true
			m, cmd := m.Update(safetyKey(key))
			if cmd != nil || m.Selection.Count() != 1 || m.filterMode || m.finder.active || m.showHidden || m.showIgnored {
				t.Fatalf("help dispatched browser key %q", key)
			}
			closed := key == "esc" || key == "q" || key == "?"
			if m.showHelp == closed {
				t.Fatalf("help visibility after %q = %v", key, m.showHelp)
			}
		})
	}
}

func TestBrowserEscapesFilenameControlsWithoutChangingSelections(t *testing.T) {
	root := t.TempDir()
	name := "file\n\x1b]52;c;SGVsbG8=\a.txt"
	path := filepath.Join(root, name)
	if err := os.WriteFile(path, []byte("content"), 0o600); err != nil {
		t.Fatal(err)
	}
	m, err := New(root)
	if err != nil {
		t.Fatal(err)
	}
	m.SetSize(200, 24)
	m.remoteHost = &config.Host{Name: "remote"}
	m.remoteEntries = m.entries
	m, _ = m.Update(tea.KeyMsg{Type: tea.KeySpace})
	if !m.Selection.IsMarked(path) || m.entries[0].Name != name {
		t.Fatal("display escaping changed the actual filename")
	}
	view := m.View()
	if strings.Count(view, "\n")+1 != m.Height || strings.Contains(view, "\x1b]52;") {
		t.Fatal("filename controls escaped into terminal layout")
	}
	if !strings.Contains(ansi.Strip(view), styles.SafeText(name)) {
		t.Fatal("escaped filename is not visible")
	}
	m.finder = finder{active: true, rel: []string{name}, abs: []string{path}}
	m.finder.recompute()
	if strings.Contains(m.View(), "\x1b]52;") || strings.Count(m.View(), "\n")+1 != m.Height {
		t.Fatal("finder emitted filename controls")
	}
}

func safetyKey(key string) tea.KeyMsg {
	switch key {
	case "esc":
		return tea.KeyMsg{Type: tea.KeyEsc}
	case "tab":
		return tea.KeyMsg{Type: tea.KeyTab}
	case "down":
		return tea.KeyMsg{Type: tea.KeyDown}
	default:
		return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
	}
}
