package browser

import "testing"

func mkFinder(rel ...string) finder {
	abs := make([]string, len(rel))
	for i, r := range rel {
		abs[i] = "/proj/" + r
	}
	return finder{active: true, rel: rel, abs: abs}
}

func TestFinderEmptyQueryReturnsAll(t *testing.T) {
	f := mkFinder("a.go", "b.go", "sub/c.go")
	f.recompute()
	if len(f.results) != 3 {
		t.Fatalf("empty query results = %d, want 3", len(f.results))
	}
	if f.results[0].rel != "a.go" || f.results[0].abs != "/proj/a.go" {
		t.Fatalf("first result = %+v", f.results[0])
	}
}

func TestFinderFuzzyMatches(t *testing.T) {
	f := mkFinder("internal/config/loader.go", "internal/diff/engine.go", "README.md")
	f.query = "cfgload"
	f.recompute()
	if len(f.results) == 0 {
		t.Fatal("expected at least one fuzzy match for 'cfgload'")
	}
	if f.results[0].rel != "internal/config/loader.go" {
		t.Fatalf("top match = %q, want internal/config/loader.go", f.results[0].rel)
	}
	if len(f.results[0].matched) == 0 {
		t.Fatal("expected matched indexes for highlighting")
	}
}

func TestFinderNoMatch(t *testing.T) {
	f := mkFinder("a.go", "b.go")
	f.query = "zzzzzz"
	f.recompute()
	if len(f.results) != 0 {
		t.Fatalf("expected no matches, got %d", len(f.results))
	}
	if f.current() != nil {
		t.Fatal("current() should be nil with no results")
	}
}

func TestFinderClamp(t *testing.T) {
	f := mkFinder("1", "2", "3", "4", "5")
	f.recompute()
	f.cursor = 99
	f.clamp(2)
	if f.cursor != 4 {
		t.Fatalf("cursor = %d, want 4", f.cursor)
	}
	if f.offset != 3 { // window of 2 -> shows rows 3,4
		t.Fatalf("offset = %d, want 3", f.offset)
	}
}
