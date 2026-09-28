package config

import (
	"errors"
	"fmt"
	"os"
	"sort"
	"strings"
)

// A project host can link to a global host instead of carrying a connection of
// its own. The global host is then a server: its hostname, port, user, auth,
// protocol and keep-alive apply to every project host that names it, while
// root_path and mappings stay with each project host. Two projects deploying
// to different directories on one machine thus share one set of credentials.
//
//	# ~/.config/drift/projects/shop-b.toml
//	[[hosts]]
//	name      = "prod"
//	server    = "kunde-x"
//	root_path = "/var/www/shop-b"
//
// A link sets no connection field of its own. Allowing it to override some of
// them would make an empty user or a zero port mean "inherited" in one record
// and "unset" in the next.

// IsLink reports whether h takes its connection from a global server.
func (h Host) IsLink() bool { return h.Server != "" }

// hasConnection reports whether h sets a field a link takes from its server.
func (h Host) hasConnection() bool {
	return h.Hostname != "" || h.Port != 0 || h.User != "" || h.Auth != (Auth{}) ||
		h.Protocol != "" || h.KeepAliveInterval != nil
}

// Resolve returns link with the connection of server, ready to connect.
func Resolve(link, server Host) Host {
	h := server
	h.Name = link.Name
	h.Server = server.Name
	h.RootPath = link.RootPath
	h.Mappings = link.Mappings
	return h
}

// linkFields drops the connection a resolved link carries in memory, leaving
// the record the project store holds.
func linkFields(h Host) Host {
	return Host{Name: h.Name, Server: h.Server, RootPath: h.RootPath, Mappings: h.Mappings}
}

// SameEndpoint reports whether a and b connect to the same account on the same
// machine: hostname, port, user and protocol. Credentials are not compared.
func SameEndpoint(a, b Host) bool {
	protocol := func(h Host) string {
		if h.Protocol == "" {
			return "sftp"
		}
		return h.Protocol
	}
	port := func(h Host) int {
		if h.Port == 0 {
			return DefaultPort(h.Protocol)
		}
		return h.Port
	}
	return strings.EqualFold(a.Hostname, b.Hostname) && a.Hostname != "" &&
		port(a) == port(b) && a.User == b.User && protocol(a) == protocol(b)
}

// ServerInUseError reports that a global server cannot be deleted or renamed
// because project hosts link to it.
type ServerInUseError struct {
	Server   string
	Projects []string // slugs of the linking projects
}

func (e *ServerInUseError) Error() string {
	return fmt.Sprintf("server %q is linked by project %s; remove those links first",
		e.Server, strings.Join(e.Projects, ", "))
}

// LinkTarget is a host outside the open project that a project host can take
// its connection from: a global server, or a host of another project, which
// becomes a global server when chosen (see PromoteProjectHost).
type LinkTarget struct {
	Project string   // slug of the project owning Host; empty for a global server
	Host    Host     // defaults applied
	UsedBy  []string // slugs of the projects linking a global server
}

// LinkTargets lists every host the open project could link to: the global
// servers first, then the unlinked hosts of the other projects.
func LinkTargets(cfg *MergedConfig) ([]LinkTarget, error) {
	stores, err := projectStores()
	if err != nil {
		return nil, err
	}

	var targets []LinkTarget
	for _, server := range SortedHostsByName(cfg.GlobalHosts) {
		targets = append(targets, LinkTarget{Host: server, UsedBy: linkingProjects(stores, server.Name)})
	}
	for _, slug := range sortedSlugs(stores) {
		if slug == cfg.ProjectSlug {
			continue
		}
		store := stores[slug]
		for _, h := range SortedHostsByName(store.Hosts) {
			if !h.IsLink() {
				targets = append(targets, LinkTarget{Project: slug, Host: withDefaults(h, store.Defaults)})
			}
		}
	}
	return targets, nil
}

// MatchingTargets returns the targets connecting to the same endpoint as h,
// global servers first.
func MatchingTargets(targets []LinkTarget, h Host) []LinkTarget {
	var matches []LinkTarget
	for _, t := range targets {
		if SameEndpoint(t.Host, h) {
			matches = append(matches, t)
		}
	}
	return matches
}

// PromoteProjectHost turns host name of project slug into a global server and
// that host into a link to it, so the open project can link to it too. It
// returns the new server.
//
// The global config is written first. If the project store cannot be written
// afterwards, the server exists as a copy of the host, which is harmless; the
// opposite order could leave a link to a server that does not exist.
func PromoteProjectHost(cfg *MergedConfig, slug, name string) (Host, error) {
	if slug == cfg.ProjectSlug {
		return Host{}, errors.New("the host belongs to the open project")
	}
	store, err := loadProjectStore(slug)
	if err != nil {
		return Host{}, fmt.Errorf("project store %s: %w", slug, err)
	}
	if store == nil {
		return Host{}, fmt.Errorf("project %s has no hosts", slug)
	}
	idx := -1
	for i, h := range store.Hosts {
		if h.Name == name {
			idx = i
		}
	}
	if idx < 0 {
		return Host{}, fmt.Errorf("project %s has no host %q", slug, name)
	}
	source := store.Hosts[idx]
	if source.IsLink() {
		return Host{}, fmt.Errorf("host %q of project %s already links server %q", name, slug, source.Server)
	}

	base, err := globalConfigBase(cfg)
	if err != nil {
		return Host{}, err
	}
	// The project's defaults would not reach the host once it is global, so
	// they become part of the server.
	server := withDefaults(source, store.Defaults)
	server.Name = freeServerName(base.Hosts, slug, name)
	server.Mappings = nil // relative to the source project, meaningless elsewhere
	base.Hosts = append(base.Hosts, server)
	if err := writeGlobal(base); err != nil {
		return Host{}, err
	}
	resolved := withDefaults(server, cfg.GlobalDefaults)
	cfg.GlobalHosts = append(cfg.GlobalHosts, resolved)
	rebuildMerged(cfg)

	store.Hosts[idx] = Host{Name: source.Name, Server: server.Name, RootPath: source.RootPath, Mappings: source.Mappings}
	if err := writeProjectStore(slug, *store); err != nil {
		return resolved, fmt.Errorf("server %q was added, but project %s keeps its own copy of the host: %w", server.Name, slug, err)
	}
	return resolved, nil
}

// freeServerName picks the host's own name for its server, the project-prefixed
// name when a server already has it, and a numbered one after that.
func freeServerName(servers []Host, slug, name string) string {
	taken := make(map[string]bool, len(servers))
	for _, s := range servers {
		taken[s.Name] = true
	}
	if !taken[name] {
		return name
	}
	candidate := slug + "-" + name
	for n := 2; taken[candidate]; n++ {
		candidate = fmt.Sprintf("%s-%s-%d", slug, name, n)
	}
	return candidate
}

// serverUsers returns the slugs of the projects linking server, read from disk
// so that projects not open in this session count too.
func serverUsers(server string) ([]string, error) {
	stores, err := projectStores()
	if err != nil {
		return nil, err
	}
	return linkingProjects(stores, server), nil
}

func linkingProjects(stores map[string]*ProjectConfig, server string) []string {
	var slugs []string
	for _, slug := range sortedSlugs(stores) {
		for _, h := range stores[slug].Hosts {
			if h.Server == server {
				slugs = append(slugs, slug)
				break
			}
		}
	}
	return slugs
}

// projectStores reads every project store, keyed by slug. A store that cannot
// be read is an error rather than a skipped file: callers use the result to
// decide whether a server is still in use.
func projectStores() (map[string]*ProjectConfig, error) {
	entries, err := os.ReadDir(projectsDir())
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	stores := make(map[string]*ProjectConfig)
	for _, e := range entries {
		name := e.Name()
		// Temporary files of an atomic write or a staged removal start with a dot.
		if e.IsDir() || strings.HasPrefix(name, ".") || !strings.HasSuffix(name, ".toml") {
			continue
		}
		slug := strings.TrimSuffix(name, ".toml")
		store, err := loadProjectStore(slug)
		if err != nil {
			return nil, fmt.Errorf("project store %s: %w", slug, err)
		}
		if store != nil {
			stores[slug] = store
		}
	}
	return stores, nil
}

func sortedSlugs(stores map[string]*ProjectConfig) []string {
	slugs := make([]string, 0, len(stores))
	for slug := range stores {
		slugs = append(slugs, slug)
	}
	sort.Strings(slugs)
	return slugs
}

// validateLinks checks the links of a project's raw hosts against the servers.
func validateLinks(hosts []Host, servers []Host) error {
	names := make(map[string]bool, len(servers))
	for _, s := range servers {
		names[s.Name] = true
	}
	for _, h := range hosts {
		if !h.IsLink() {
			continue
		}
		if h.hasConnection() {
			return fmt.Errorf("host %q links server %q and must not set hostname, port, user, auth, protocol or keep_alive_interval", h.Name, h.Server)
		}
		if !names[h.Server] {
			return fmt.Errorf("host %q links server %q, which is not a global host", h.Name, h.Server)
		}
	}
	return nil
}
