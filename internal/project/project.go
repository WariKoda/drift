// Package project manages the global registry of drift projects.
//
// A project is a named pointer to a local directory whose hosts and mappings
// live in <path>/.drift/config.toml (the existing per-project config mechanism).
// The registry itself only stores slug, display name, path and timestamps, and
// is persisted to <config-dir>/projects.toml.
package project

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"unicode"
)

// Project is a single registry entry.
type Project struct {
	Slug      string    `toml:"slug"` // unique, derived from Name
	Name      string    `toml:"name"` // display name, e.g. "KUNDE A"
	Path      string    `toml:"path"` // absolute local path
	Archived  bool      `toml:"archived,omitempty"`
	CreatedAt time.Time `toml:"created_at"`
	UpdatedAt time.Time `toml:"updated_at"`
}

// Registry is the structure of projects.toml.
type Registry struct {
	Projects []Project `toml:"projects"`
}

// Find returns a pointer to the project with the given slug, or nil.
func (r *Registry) Find(slug string) *Project {
	for i := range r.Projects {
		if r.Projects[i].Slug == slug {
			return &r.Projects[i]
		}
	}
	return nil
}

// HasPath reports whether any project (cleaned) points at the given path.
func (r *Registry) HasPath(path string) bool {
	cp := filepath.Clean(path)
	for i := range r.Projects {
		if filepath.Clean(r.Projects[i].Path) == cp {
			return true
		}
	}
	return false
}

// Add appends a project. It errors if the slug already exists.
func (r *Registry) Add(p Project) error {
	if p.Slug == "" {
		return fmt.Errorf("project slug must not be empty")
	}
	if r.Find(p.Slug) != nil {
		return fmt.Errorf("project %q already exists", p.Slug)
	}
	r.Projects = append(r.Projects, p)
	return nil
}

// Update replaces the project identified by slug. It errors if not found.
func (r *Registry) Update(slug string, p Project) error {
	for i := range r.Projects {
		if r.Projects[i].Slug == slug {
			r.Projects[i] = p
			return nil
		}
	}
	return fmt.Errorf("project %q not found", slug)
}

// Remove deletes the project identified by slug. It errors if not found.
func (r *Registry) Remove(slug string) error {
	for i := range r.Projects {
		if r.Projects[i].Slug == slug {
			r.Projects = append(r.Projects[:i], r.Projects[i+1:]...)
			return nil
		}
	}
	return fmt.Errorf("project %q not found", slug)
}

// Active returns non-archived projects sorted by display name (then slug).
func (r *Registry) Active() []Project {
	return r.sorted(false)
}

// All returns every project, including archived ones, sorted by name (then slug).
func (r *Registry) All() []Project {
	return r.sorted(true)
}

func (r *Registry) sorted(includeArchived bool) []Project {
	out := make([]Project, 0, len(r.Projects))
	for _, p := range r.Projects {
		if !includeArchived && p.Archived {
			continue
		}
		out = append(out, p)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Name != out[j].Name {
			return out[i].Name < out[j].Name
		}
		return out[i].Slug < out[j].Slug
	})
	return out
}

// Slugify converts a display name into a URL-friendly slug:
// lowercase ASCII, spaces and runs of invalid characters collapsed to a single "-".
func Slugify(name string) string {
	var b strings.Builder
	prevDash := false
	for _, r := range strings.ToLower(strings.TrimSpace(name)) {
		switch {
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9':
			b.WriteRune(r)
			prevDash = false
		case unicode.IsSpace(r) || r == '-' || r == '_':
			if !prevDash && b.Len() > 0 {
				b.WriteByte('-')
				prevDash = true
			}
		default:
			// drop accents/punctuation; treat as a separator boundary
			if !prevDash && b.Len() > 0 {
				b.WriteByte('-')
				prevDash = true
			}
		}
	}
	return strings.Trim(b.String(), "-")
}

// UniqueSlug returns base if unused, otherwise base-2, base-3, … until free.
func (r *Registry) UniqueSlug(base string) string {
	if base == "" {
		base = "project"
	}
	if r.Find(base) == nil {
		return base
	}
	for i := 2; ; i++ {
		candidate := fmt.Sprintf("%s-%d", base, i)
		if r.Find(candidate) == nil {
			return candidate
		}
	}
}
