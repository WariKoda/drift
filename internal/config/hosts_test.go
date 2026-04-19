package config

import "testing"

func TestSortedHostsByName(t *testing.T) {
	input := []Host{
		{Name: "prod", Hostname: "z.example.com", Port: 22},
		{Name: "alpha", Hostname: "b.example.com", Port: 22},
		{Name: "alpha", Hostname: "a.example.com", Port: 22},
	}

	got := SortedHostsByName(input)

	if len(got) != 3 {
		t.Fatalf("len(SortedHostsByName) = %d, want 3", len(got))
	}
	if got[0].Hostname != "a.example.com" {
		t.Fatalf("got[0].Hostname = %q, want %q", got[0].Hostname, "a.example.com")
	}
	if got[1].Hostname != "b.example.com" {
		t.Fatalf("got[1].Hostname = %q, want %q", got[1].Hostname, "b.example.com")
	}
	if got[2].Name != "prod" {
		t.Fatalf("got[2].Name = %q, want %q", got[2].Name, "prod")
	}

	if input[0].Name != "prod" {
		t.Fatal("SortedHostsByName mutated input slice")
	}
}
