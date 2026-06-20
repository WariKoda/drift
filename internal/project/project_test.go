package project

import "testing"

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
