package tui

import (
	"errors"
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
)

// linkServer returns the global server behind target, promoting a host of
// another project first. A server with a name and an error means the promotion
// wrote the server but could not turn the source host into a link; the server
// is usable, the error still has to be shown.
func (a *App) linkServer(target config.LinkTarget) (config.Host, error) {
	if target.Project == "" {
		return target.Host, nil
	}
	server, err := config.PromoteProjectHost(a.state.Config, target.Project, target.Host.Name)
	if err != nil {
		log.Error("promote project host failed", "project", target.Project, "host", target.Host.Name, "err", err)
	}
	return server, err
}

// sameEndpoint finds a host outside the open project that connects where h
// does. It is a hint, so a store that cannot be read is logged, not shown in
// place of the save.
func (a *App) sameEndpoint(h config.Host) (config.LinkTarget, bool) {
	targets, err := config.LinkTargets(a.state.Config)
	if err != nil {
		log.Error("list link targets failed", "err", err)
		return config.LinkTarget{}, false
	}
	matches := config.MatchingTargets(targets, h)
	if len(matches) == 0 {
		return config.LinkTarget{}, false
	}
	return matches[0], true
}

// freeProjectHostName is name, or name with a number when the open project
// already has a host called that.
func (a *App) freeProjectHostName(name string) string {
	taken := make(map[string]bool, len(a.state.Config.ProjectHosts))
	for _, h := range a.state.Config.ProjectHosts {
		taken[h.Name] = true
	}
	candidate := name
	for n := 2; taken[candidate]; n++ {
		candidate = fmt.Sprintf("%s-%d", name, n)
	}
	return candidate
}

// projectNames maps registry slugs to project names.
func (a *App) projectNames() map[string]string {
	names := make(map[string]string)
	if a.registry == nil {
		return names
	}
	for _, p := range a.registry.Projects {
		names[p.Slug] = p.Name
	}
	return names
}

func (a *App) projectName(slug string) string {
	if name := a.projectNames()[slug]; name != "" {
		return name
	}
	return slug
}

// describeTarget names target for the offer to link it.
func (a *App) describeTarget(target config.LinkTarget) string {
	if target.Project == "" {
		return fmt.Sprintf("global server %q", target.Host.Name)
	}
	return fmt.Sprintf("%q in project %s", target.Host.Name, a.projectName(target.Project))
}

// describeConfigError names projects by name rather than slug.
func (a *App) describeConfigError(err error) string {
	var inUse *config.ServerInUseError
	if !errors.As(err, &inUse) {
		return err.Error()
	}
	names := make([]string, len(inUse.Projects))
	for i, slug := range inUse.Projects {
		names[i] = a.projectName(slug)
	}
	return fmt.Sprintf("server %q is linked by %s; remove those links first", inUse.Server, strings.Join(names, ", "))
}
