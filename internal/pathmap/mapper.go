// Package pathmap resolves local filesystem paths to remote paths and vice versa
// using the Mapping rules from the project config.
package pathmap

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/yourusername/drift/internal/config"
)

// Mapper translates between local absolute paths and remote absolute paths.
type Mapper struct {
	projectRoot string
	mappings    []config.Mapping
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

// LocalToRemote converts an absolute local path to an absolute remote path.
// It uses the longest matching mapping prefix. Falls back to host.RootPath + relative path.
func (m *Mapper) LocalToRemote(absLocal string) (string, error) {
	absLocal = filepath.Clean(absLocal)

	best := ""
	bestMapping := config.Mapping{}

	for _, mp := range m.mappings {
		localBase := filepath.Join(m.projectRoot, filepath.FromSlash(mp.Local))
		localBase = filepath.Clean(localBase)

		if strings.HasPrefix(absLocal, localBase) {
			if len(localBase) > len(best) {
				best = localBase
				bestMapping = mp
			}
		}
	}

	var remoteBase string
	var relSuffix string

	if best != "" {
		// matched a mapping
		remoteLocal := filepath.ToSlash(strings.TrimPrefix(absLocal, best))
		remoteLocal = strings.TrimPrefix(remoteLocal, "/")
		remoteBase = strings.TrimSuffix(
			filepath.ToSlash(filepath.Join(m.host.RootPath, bestMapping.Remote)),
			"/",
		)
		relSuffix = remoteLocal
	} else {
		// fallback: use path relative to project root
		rel, err := filepath.Rel(m.projectRoot, absLocal)
		if err != nil {
			return "", fmt.Errorf("pathmap: cannot relativize %q against project root %q: %w", absLocal, m.projectRoot, err)
		}
		remoteBase = strings.TrimSuffix(filepath.ToSlash(m.host.RootPath), "/")
		relSuffix = filepath.ToSlash(rel)
	}

	if relSuffix == "" || relSuffix == "." {
		return remoteBase, nil
	}
	return remoteBase + "/" + relSuffix, nil
}

// RemoteToLocal converts an absolute remote path to an absolute local path.
func (m *Mapper) RemoteToLocal(absRemote string) (string, error) {
	absRemote = filepath.ToSlash(absRemote)

	best := ""
	bestMapping := config.Mapping{}

	for _, mp := range m.mappings {
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

	// fallback
	rootPath := strings.TrimSuffix(filepath.ToSlash(m.host.RootPath), "/")
	if !strings.HasPrefix(absRemote, rootPath) {
		return "", fmt.Errorf("pathmap: remote path %q is outside host root %q", absRemote, m.host.RootPath)
	}
	rel := strings.TrimPrefix(absRemote, rootPath)
	rel = strings.TrimPrefix(rel, "/")
	return filepath.Join(m.projectRoot, rel), nil
}
