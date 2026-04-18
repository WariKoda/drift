// Package pathmap resolves local filesystem paths to remote paths and vice versa
// using the Mapping rules from the project config.
package pathmap

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/config"
)

// Mapper translates between local absolute paths and remote absolute paths.
type Mapper struct {
	projectRoot string
	mappings    []config.Mapping // project-level fallback
	host        config.Host
}

// New creates a Mapper for the given host and project mappings.
func New(projectRoot string, mappings []config.Mapping, host config.Host) *Mapper {
	return &Mapper{
		projectRoot: filepath.Clean(projectRoot),
		mappings:    mappings,
		host:        host,
	}
}

// activeMappings returns the effective mapping list.
// Host-level mappings take precedence over project-level mappings.
func (m *Mapper) activeMappings() []config.Mapping {
	if len(m.host.Mappings) > 0 {
		return m.host.Mappings
	}
	return m.mappings
}

// LocalToRemote converts an absolute local path to an absolute remote path.
// When any effective mappings are configured, the file must match one of them.
// Falls back to host.RootPath + relative path when no mappings are configured.
func (m *Mapper) LocalToRemote(absLocal string) (string, error) {
	absLocal = filepath.Clean(absLocal)

	mappings := m.activeMappings()
	best := ""
	bestMapping := config.Mapping{}

	for _, mp := range mappings {
		localBase := filepath.Join(m.projectRoot, filepath.FromSlash(mp.Local))
		localBase = filepath.Clean(localBase)

		if strings.HasPrefix(absLocal, localBase) {
			if len(localBase) > len(best) {
				best = localBase
				bestMapping = mp
			}
		}
	}

	if best != "" {
		remoteLocal := filepath.ToSlash(strings.TrimPrefix(absLocal, best))
		remoteLocal = strings.TrimPrefix(remoteLocal, "/")
		remoteBase := strings.TrimSuffix(
			filepath.ToSlash(filepath.Join(m.host.RootPath, bestMapping.Remote)),
			"/",
		)
		if remoteLocal == "" || remoteLocal == "." {
			return remoteBase, nil
		}
		return remoteBase + "/" + remoteLocal, nil
	}

	// Mappings are configured but no match → file is not configured for sync
	if len(mappings) > 0 {
		return "", fmt.Errorf("%s: not covered by any configured mapping", filepath.Base(absLocal))
	}

	// Fallback: use path relative to project root
	rel, err := filepath.Rel(m.projectRoot, absLocal)
	if err != nil {
		return "", fmt.Errorf("pathmap: cannot relativize %q against project root %q: %w", absLocal, m.projectRoot, err)
	}
	remoteBase := strings.TrimSuffix(filepath.ToSlash(m.host.RootPath), "/")
	relSuffix := filepath.ToSlash(rel)
	if relSuffix == "" || relSuffix == "." {
		return remoteBase, nil
	}
	return remoteBase + "/" + relSuffix, nil
}

// RemoteToLocal converts an absolute remote path to an absolute local path.
func (m *Mapper) RemoteToLocal(absRemote string) (string, error) {
	absRemote = filepath.ToSlash(absRemote)

	mappings := m.activeMappings()
	best := ""
	bestMapping := config.Mapping{}

	for _, mp := range mappings {
		remoteBase := strings.TrimSuffix(
			filepath.ToSlash(filepath.Join(m.host.RootPath, mp.Remote)),
			"/",
		)
		if strings.HasPrefix(absRemote, remoteBase) {
			if len(remoteBase) > len(best) {
				best = remoteBase
				bestMapping = mp
			}
		}
	}

	if best != "" {
		suffix := strings.TrimPrefix(absRemote, best)
		suffix = strings.TrimPrefix(suffix, "/")
		localBase := filepath.Join(m.projectRoot, filepath.FromSlash(bestMapping.Local))
		if suffix == "" {
			return filepath.Clean(localBase), nil
		}
		return filepath.Join(localBase, suffix), nil
	}

	// Mappings are configured but no match
	if len(mappings) > 0 {
		return "", fmt.Errorf("pathmap: remote path %q not covered by any configured mapping", absRemote)
	}

	// Fallback
	rootPath := strings.TrimSuffix(filepath.ToSlash(m.host.RootPath), "/")
	if !strings.HasPrefix(absRemote, rootPath) {
		return "", fmt.Errorf("pathmap: remote path %q is outside host root %q", absRemote, m.host.RootPath)
	}
	rel := strings.TrimPrefix(absRemote, rootPath)
	rel = strings.TrimPrefix(rel, "/")
	return filepath.Join(m.projectRoot, rel), nil
}
