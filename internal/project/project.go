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
	UpdatedAt time.Time `toml:"updated_at"` // edits and archive only; not bumped on open
	OpenedAt  time.Time `toml:"opened_at,omitempty"`
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

// FindByPath returns the project whose cleaned Path equals cleaned path, or nil.
func (r *Registry) FindByPath(path string) *Project {
	cp := filepath.Clean(path)
	for i := range r.Projects {
		if filepath.Clean(r.Projects[i].Path) == cp {
			return &r.Projects[i]
		}
	}
	return nil
}

// HasPath reports whether any project (cleaned) points at the given path.
func (r *Registry) HasPath(path string) bool {
	return r.FindByPath(path) != nil
}

// MostRecentlyOpened returns the non-archived project with the latest non-zero
// OpenedAt, or nil.
func (r *Registry) MostRecentlyOpened() *Project {
	for _, p := range r.Active() {
		if !p.OpenedAt.IsZero() {
			return r.Find(p.Slug)
		}
	}
	return nil
}

// Match finds a project by user query for `drift open`.
// Resolution order:
//  1. exact slug
//  2. exact display name, case-insensitive; error if more than one
//  3. unique case-insensitive prefix of slug or name
//  4. unique case-insensitive substring of slug or name
//
// Error if none or ambiguous. Search includes archived projects.
func (r *Registry) Match(query string) (*Project, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, fmt.Errorf("project query must not be empty")
	}
	if p := r.Find(query); p != nil {
		return p, nil
	}

	var names []*Project
	for i := range r.Projects {
		if strings.EqualFold(r.Projects[i].Name, query) {
			names = append(names, &r.Projects[i])
		}
	}
	if p, err := pickUnique(query, names); p != nil || err != nil {
		return p, err
	}

	q := strings.ToLower(query)
	var prefix []*Project
	for i := range r.Projects {
		p := &r.Projects[i]
		if strings.HasPrefix(strings.ToLower(p.Slug), q) || strings.HasPrefix(strings.ToLower(p.Name), q) {
			prefix = append(prefix, p)
		}
	}
	if p, err := pickUnique(query, prefix); p != nil || err != nil {
		return p, err
	}

	var sub []*Project
	for i := range r.Projects {
		p := &r.Projects[i]
		if strings.Contains(strings.ToLower(p.Slug), q) || strings.Contains(strings.ToLower(p.Name), q) {
			sub = append(sub, p)
		}
	}
	if p, err := pickUnique(query, sub); p != nil || err != nil {
		return p, err
	}
	return nil, fmt.Errorf("no project matching %q", query)
}

// pickUnique returns the sole hit, an ambiguous error, or (nil, nil) when empty.
func pickUnique(query string, hits []*Project) (*Project, error) {
	switch len(hits) {
	case 0:
		return nil, nil
	case 1:
		return hits[0], nil
	}
	slugs := make([]string, len(hits))
	for i, p := range hits {
		slugs[i] = p.Slug
	}
	sort.Strings(slugs)
	return nil, fmt.Errorf("ambiguous project %q: matches %s", query, strings.Join(slugs, ", "))
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

// Active returns non-archived projects, most recently opened first.
// Never-opened projects sort after those with a timestamp, by name then slug.
func (r *Registry) Active() []Project {
	return r.sorted(false)
}

// All returns every project, including archived ones, in the same order as Active.
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
		ai, aj := out[i].OpenedAt, out[j].OpenedAt
		zi, zj := ai.IsZero(), aj.IsZero()
		switch {
		case !zi && zj:
			return true
		case zi && !zj:
			return false
		case !zi && !zj && !ai.Equal(aj):
			return ai.After(aj)
		}
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
