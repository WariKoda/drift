package app

import (
	"slices"
	"testing"

	"github.com/WariKoda/drift/internal/diff"
)

func TestSortSessionsListsEachDirectoryOnce(t *testing.T) {
	sessions := []diff.Session{
		{LocalPath: "/p/src/sub/b.go"},
		{LocalPath: "/p/src2/c.go"},
		{LocalPath: "/p/src/z.go"},
		{LocalPath: "/p/README.md"},
		{LocalPath: "/p/src-x/d.go"},
		{RemotePath: "/remote/src/error.go"},
		{LocalPath: "/p/src/a.go"},
		{LocalPath: "/p/src/a.go", RemotePath: "/second"},
	}
	sortSessions(sessions)
	var got []string
	for _, session := range sessions {
		got = append(got, session.LocalPath+session.RemotePath)
	}
	want := []string{
		"/p/README.md",
		"/p/src/a.go",
		"/p/src/a.go/second",
		"/p/src/z.go",
		"/p/src/sub/b.go",
		"/p/src-x/d.go",
		"/p/src2/c.go",
		"/remote/src/error.go",
	}
	if !slices.Equal(got, want) {
		t.Fatalf("order = %q, want %q", got, want)
	}
}
