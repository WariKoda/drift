package browser

import (
	"fmt"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/charmbracelet/x/ansi"
)

func TestDeepUnicodeEntriesFitPane(t *testing.T) {
	m := Model{}
	for _, width := range []int{10, 20, 29, 40, 80} {
		for _, depth := range []int{0, 5, 20, 1000000} {
			for _, name := range []string{"file.txt", "文件名文件名文件名", "👩‍💻é👩‍💻é"} {
				entry := &fs.FileEntry{Name: name, Path: "/file", Depth: depth}
				row := m.renderEntry(entry, true, width, nil, "")
				if cells := ansi.StringWidth(row); cells != width {
					t.Fatalf("width=%d depth=%d name=%q rendered %d columns", width, depth, name, cells)
				}
			}
		}
	}
}

func TestDeepTreeViewKeepsDividerInsideTerminal(t *testing.T) {
	m := Model{WorkDir: "/project", Selection: fs.NewSelectionState(), RemoteSelection: fs.NewSelectionState()}
	entry := &fs.FileEntry{Name: "文件.txt", Path: "/file", Depth: 100}
	m.entries, m.remoteEntries = []*fs.FileEntry{entry}, []*fs.FileEntry{entry}
	m.SetSize(60, 24)
	for _, host := range []*config.Host{nil, {Name: "界remote"}} {
		m.remoteHost = host
		for _, loading := range []bool{false, true} {
			m.remoteLoading, m.remoteStatus = loading, strings.Repeat("界", 60)
			for _, row := range strings.Split(m.View(), "\n") {
				if cells := ansi.StringWidth(row); cells != 60 {
					t.Fatalf("tree row occupies %d columns instead of 60: %q", cells, ansi.Strip(row))
				}
			}
		}
	}
}

func BenchmarkLargeRemoteView(b *testing.B) {
	m := Model{WorkDir: "/project", Selection: fs.NewSelectionState(), RemoteSelection: fs.NewSelectionState(), remoteHost: &config.Host{Name: "remote"}}
	for i := 0; i < 10000; i++ {
		m.remoteEntries = append(m.remoteEntries, &fs.FileEntry{Name: fmt.Sprintf("file-%05d.txt", i), Path: fmt.Sprintf("/file-%05d.txt", i)})
	}
	m.SetSize(120, 54)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = m.View()
	}
}
