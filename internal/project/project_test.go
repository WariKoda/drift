package project

import (
	"path/filepath"
	"testing"
	"time"
)

func TestSlugify(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want string
	}{
		{"simple", "KUNDE A", "kunde-a"},
		{"already slug", "kunde-a", "kunde-a"},
		{"trim and collapse", "  Big   Client  ", "big-client"},
		{"underscores", "client_one", "client-one"},
		{"punctuation", "Acme, Inc.", "acme-inc"},
		{"accents dropped", "Café Über", "caf-ber"},
		{"leading/trailing junk", "--Hello--", "hello"},
		{"digits kept", "Project 42", "project-42"},
		{"empty", "   ", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := Slugify(tt.in); got != tt.want {
				t.Fatalf("Slugify(%q) = %q, want %q", tt.in, got, tt.want)
			}
		})
	}
}

func TestUniqueSlug(t *testing.T) {
	r := &Registry{Projects: []Project{
		{Slug: "kunde-a"},
		{Slug: "kunde-a-2"},
	}}
	if got := r.UniqueSlug("kunde-b"); got != "kunde-b" {
		t.Fatalf("free slug: got %q, want kunde-b", got)
	}
	if got := r.UniqueSlug("kunde-a"); got != "kunde-a-3" {
		t.Fatalf("collision: got %q, want kunde-a-3", got)
	}
	if got := r.UniqueSlug(""); got != "project" {
		t.Fatalf("empty base: got %q, want project", got)
	}
}

func TestAddUpdateRemoveFind(t *testing.T) {
	r := &Registry{}
	if err := r.Add(Project{Slug: "a", Name: "A"}); err != nil {
		t.Fatalf("Add: %v", err)
	}
	if err := r.Add(Project{Slug: "a", Name: "dup"}); err == nil {
		t.Fatal("Add duplicate slug should error")
	}
	if err := r.Add(Project{Slug: "", Name: "x"}); err == nil {
		t.Fatal("Add empty slug should error")
	}
	if p := r.Find("a"); p == nil || p.Name != "A" {
		t.Fatalf("Find: got %v", p)
	}
	if err := r.Update("a", Project{Slug: "a", Name: "A2"}); err != nil {
		t.Fatalf("Update: %v", err)
	}
	if p := r.Find("a"); p == nil || p.Name != "A2" {
		t.Fatalf("after Update: got %v", p)
	}
	if err := r.Update("missing", Project{Slug: "missing"}); err == nil {
		t.Fatal("Update missing should error")
	}
	if err := r.Remove("a"); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if r.Find("a") != nil {
		t.Fatal("project still present after Remove")
	}
	if err := r.Remove("a"); err == nil {
		t.Fatal("Remove missing should error")
	}
}

func TestHasPath(t *testing.T) {
	r := &Registry{Projects: []Project{
		{Slug: "a", Path: "/home/u/work/kunde-a"},
		{Slug: "b", Path: "/home/u/work/kunde-b/"},
	}}
	if !r.HasPath("/home/u/work/kunde-a") {
		t.Fatal("expected exact path to match")
	}
	if !r.HasPath("/home/u/work/kunde-b") {
		t.Fatal("expected trailing-slash path to match after Clean")
	}
	if r.HasPath("/home/u/work/kunde-c") {
		t.Fatal("unregistered path should not match")
	}
}

func TestActiveAndAllSorting(t *testing.T) {
	r := &Registry{Projects: []Project{
		{Slug: "z", Name: "Zeta"},
		{Slug: "arch", Name: "Archived", Archived: true},
		{Slug: "a", Name: "Alpha"},
	}}

	active := r.Active()
	if len(active) != 2 {
		t.Fatalf("Active len = %d, want 2", len(active))
	}
	if active[0].Name != "Alpha" || active[1].Name != "Zeta" {
		t.Fatalf("Active not sorted: %v", active)
	}

	all := r.All()
	if len(all) != 3 {
		t.Fatalf("All len = %d, want 3", len(all))
	}
	if all[0].Name != "Alpha" || all[1].Name != "Archived" || all[2].Name != "Zeta" {
		t.Fatalf("All not sorted: %v", all)
	}
}

func TestSortingByOpenedAt(t *testing.T) {
	recent := time.Date(2026, 6, 1, 12, 0, 0, 0, time.UTC)
	older := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	r := &Registry{Projects: []Project{
		{Slug: "z", Name: "Zeta", OpenedAt: older},
		{Slug: "a", Name: "Alpha"},
		{Slug: "b", Name: "Beta", OpenedAt: recent},
		{Slug: "arch", Name: "Archived", Archived: true, OpenedAt: recent.Add(time.Hour)},
	}}

	active := r.Active()
	if len(active) != 3 {
		t.Fatalf("Active len = %d, want 3", len(active))
	}
	if active[0].Name != "Beta" || active[1].Name != "Zeta" || active[2].Name != "Alpha" {
		t.Fatalf("Active order = %s, %s, %s; want Beta, Zeta, Alpha",
			active[0].Name, active[1].Name, active[2].Name)
	}

	if p := r.MostRecentlyOpened(); p == nil || p.Slug != "b" {
		t.Fatalf("MostRecentlyOpened = %v, want Beta (archived has a later OpenedAt)", p)
	}

	same := time.Date(2026, 3, 1, 0, 0, 0, 0, time.UTC)
	tied := &Registry{Projects: []Project{
		{Slug: "z", Name: "Same", OpenedAt: same},
		{Slug: "a", Name: "Same", OpenedAt: same},
	}}
	got := tied.Active()
	if got[0].Slug != "a" || got[1].Slug != "z" {
		t.Fatalf("equal OpenedAt should tie-break by slug: %s, %s", got[0].Slug, got[1].Slug)
	}

	none := &Registry{Projects: []Project{
		{Slug: "a", Name: "A"},
		{Slug: "arch", Name: "Archived", Archived: true, OpenedAt: recent},
	}}
	if p := none.MostRecentlyOpened(); p != nil {
		t.Fatalf("MostRecentlyOpened with only archived/zero = %v, want nil", p)
	}
}

func TestFindByPath(t *testing.T) {
	r := &Registry{Projects: []Project{
		{Slug: "a", Path: "/home/u/work/kunde-a"},
		{Slug: "b", Path: "/home/u/work/kunde-b/"},
	}}
	if p := r.FindByPath("/home/u/work/kunde-a"); p == nil || p.Slug != "a" {
		t.Fatalf("exact path: got %v", p)
	}
	if p := r.FindByPath("/home/u/work/kunde-b"); p == nil || p.Slug != "b" {
		t.Fatal("expected trailing-slash path to match after Clean")
	}
	if p := r.FindByPath(filepath.Clean("/home/u/work/kunde-a/../kunde-a")); p == nil || p.Slug != "a" {
		t.Fatalf("cleaned relative segments: got %v", p)
	}
	if p := r.FindByPath("/home/u/work/kunde-c"); p != nil {
		t.Fatal("unregistered path should not match")
	}
}

func TestMatch(t *testing.T) {
	r := &Registry{Projects: []Project{
		{Slug: "kunde-a", Name: "KUNDE A"},
		{Slug: "kunde-b", Name: "KUNDE B"},
		{Slug: "acme", Name: "Acme Corp"},
		{Slug: "old-acme", Name: "Legacy", Archived: true},
		{Slug: "shop-1", Name: "Shop"},
		{Slug: "shop-2", Name: "Shop"},
	}}

	tests := []struct {
		name    string
		query   string
		want    string
		wantErr bool
	}{
		{"exact slug", "kunde-a", "kunde-a", false},
		{"exact slug wins over substring", "acme", "acme", false},
		{"exact name", "KUNDE A", "kunde-a", false},
		{"exact name case-insensitive", "kunde a", "kunde-a", false},
		{"trim space", "  acme  ", "acme", false},
		{"unique prefix slug", "acm", "acme", false},
		{"unique prefix name", "acme c", "acme", false},
		{"unique substring", "corp", "acme", false},
		{"archived exact slug", "old-acme", "old-acme", false},
		{"archived exact name", "legacy", "old-acme", false},
		{"ambiguous prefix", "kunde", "", true},
		{"ambiguous exact name", "Shop", "", true},
		{"ambiguous substring", "unde", "", true},
		{"missing", "nope", "", true},
		{"empty", "", "", true},
		{"whitespace", "   ", "", true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p, err := r.Match(tt.query)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("Match(%q) = %v, want error", tt.query, p)
				}
				return
			}
			if err != nil {
				t.Fatalf("Match(%q): %v", tt.query, err)
			}
			if p.Slug != tt.want {
				t.Fatalf("Match(%q) = %q, want %q", tt.query, p.Slug, tt.want)
			}
		})
	}
}

func TestFindByPathPrefix(t *testing.T) {
	reg := &Registry{Projects: []Project{
		{Slug: "outer", Name: "Outer", Path: "/work/shop"},
		{Slug: "inner", Name: "Inner", Path: "/work/shop/plugins/one"},
		{Slug: "other", Name: "Other", Path: "/work/other"},
	}}

	cases := []struct {
		dir  string
		want string
	}{
		{"/work/shop", "outer"},
		{"/work/shop/src/deep", "outer"},
		{"/work/shop/plugins/one", "inner"},
		{"/work/shop/plugins/one/src", "inner"}, // longest match wins
		{"/work/other", "other"},
		{"/work", ""},
		{"/work/shopping", ""}, // a path prefix is not a directory prefix
		{"/elsewhere", ""},
	}
	for _, tc := range cases {
		got := reg.FindByPathPrefix(tc.dir)
		if tc.want == "" {
			if got != nil {
				t.Fatalf("FindByPathPrefix(%q) = %q, want no project", tc.dir, got.Slug)
			}
			continue
		}
		if got == nil || got.Slug != tc.want {
			t.Fatalf("FindByPathPrefix(%q) = %v, want %q", tc.dir, got, tc.want)
		}
	}
}
