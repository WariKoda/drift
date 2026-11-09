package diffview

import (
	"fmt"
	"reflect"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
)

func displayFixture(n, every int) *diff.DiffResult {
	lines := make([]diff.DiffLine, n)
	for i := range lines {
		lines[i] = diff.DiffLine{Text: "the quick brown fox jumps over the lazy dog", LocalNum: i + 1, RemoteNum: i + 1}
		if i%every == every/2 {
			lines[i].Kind = diff.LineAdded
		}
	}
	return &diff.DiffResult{Lines: lines}
}

func TestDisplayCacheReusedAndInvalidated(t *testing.T) {
	result := displayFixture(1000, 100)
	m := New([]diff.Session{{Result: result}}, config.Host{}, nil, nil, 120, 40)
	cache := m.display
	_ = m.View()
	m, _ = m.handleKey(keyMsg("j"))
	m.SetSize(100, 30)
	if m.display != cache {
		t.Fatal("view, scroll or resize rebuilt rows")
	}
	m.toggleAllFolds()
	if m.display == cache {
		t.Fatal("fold change reused stale rows")
	}
	m.syncDirs[0] = DirUpload
	m.prepareDisplayRows()
	if !m.display.flip {
		t.Fatal("direction did not invalidate cache")
	}
	fresh := diff.Flatten(result.Lines, diff.DefaultContext, m.expandedFor(0), true)
	diff.LabelHunks(result.Lines, fresh, true, "")
	if !reflect.DeepEqual(m.displayRows(), fresh) {
		t.Fatal("cache differs from fresh plan")
	}
	m.sessions[0].Result = displayFixture(200, 10)
	m.prepareDisplayRows()
	if m.display.result == result {
		t.Fatal("new comparison reused old plan")
	}
}

func BenchmarkDiffView(b *testing.B) {
	for _, n := range []int{1000, 20000, 40000} {
		for _, every := range []int{2, 100} {
			b.Run(fmt.Sprintf("lines=%d/changeEvery=%d", n, every), func(b *testing.B) {
				m := New([]diff.Session{{Result: displayFixture(n, every)}}, config.Host{}, nil, nil, 120, 40)
				b.ReportAllocs()
				b.ResetTimer()
				for i := 0; i < b.N; i++ {
					_ = m.View()
				}
			})
		}
	}
}

func BenchmarkFlatten(b *testing.B) {
	for _, n := range []int{1000, 20000, 40000} {
		b.Run(fmt.Sprintf("lines=%d", n), func(b *testing.B) {
			result := displayFixture(n, 100)
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				_ = diff.Flatten(result.Lines, diff.DefaultContext, nil, false)
			}
		})
	}
}

// BenchmarkContinuous separates the per-change plan rebuild from per-frame
// drawing, which must stay independent of the number of files.
func BenchmarkContinuous(b *testing.B) {
	for _, files := range []int{20, 200} {
		sessions := make([]diff.Session, files)
		for i := range sessions {
			sessions[i] = diff.Session{LocalPath: fmt.Sprintf("/p/%03d.txt", i), Result: displayFixture(1000, 100)}
		}
		m := New(sessions, config.Host{}, nil, nil, 120, 40)
		m.SetPreferences(config.DiffPreferences{Continuous: true})
		m.scroll = m.totalLines() / 2
		b.Run(fmt.Sprintf("files=%d/plan", files), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				_ = m.buildStream()
			}
		})
		b.Run(fmt.Sprintf("files=%d/view", files), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				_ = m.View()
			}
		})
	}
}
