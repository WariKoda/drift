package diff

import (
	"fmt"
	"strings"
	"testing"
)

// genTextPair builds two versions of an n-line text where roughly changeFrac of
// the lines differ between local and remote. It exercises both the equal-run and
// the delete+insert (modification) paths of lineDiff.
func genTextPair(n int, changeFrac float64) (local, remote string) {
	var lb, rb strings.Builder
	for i := 0; i < n; i++ {
		line := fmt.Sprintf("line %d: the quick brown fox jumps over\n", i)
		lb.WriteString(line)
		if float64(i%100)/100.0 < changeFrac {
			rb.WriteString(fmt.Sprintf("line %d: CHANGED content here now\n", i))
		} else {
			rb.WriteString(line)
		}
	}
	return lb.String(), rb.String()
}

func BenchmarkLineDiff(b *testing.B) {
	cases := []struct {
		lines      int
		changeFrac float64
	}{
		{100, 0.01},
		{100, 0.50},
		{1_000, 0.01},
		{1_000, 0.50},
		{20_000, 0.01},
		{20_000, 0.50},
	}
	for _, c := range cases {
		local, remote := genTextPair(c.lines, c.changeFrac)
		b.Run(fmt.Sprintf("lines=%d/changed=%.0f%%", c.lines, c.changeFrac*100), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				_ = lineDiff(local, remote)
			}
		})
	}
}

// BenchmarkResultStats measures HasDiff + Counts, which run per file during load
// (HasDiff is called once per compared file) and previously per UI frame.
func BenchmarkResultStats(b *testing.B) {
	local, remote := genTextPair(20_000, 0.10)
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		// Fresh result each iteration so the cache is populated from cold,
		// matching the cost paid once per loaded file.
		r := &DiffResult{Lines: lineDiff(local, remote)}
		if !r.HasDiff() {
			b.Fatal("expected diff")
		}
		_, _ = r.Counts()
	}
}

// BenchmarkResultStatsCached measures repeated queries on an already-scanned
// result — the per-frame path. Should be allocation-free and O(1).
func BenchmarkResultStatsCached(b *testing.B) {
	local, remote := genTextPair(20_000, 0.10)
	r := &DiffResult{Lines: lineDiff(local, remote)}
	r.HasDiff() // warm the cache
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = r.HasDiff()
		_, _ = r.Counts()
	}
}
