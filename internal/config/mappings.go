package config

import (
	"fmt"
	"path"
	"path/filepath"
	"strings"
)

// ValidateMappings verifies that mapping paths stay relative to their roots and
// that multiple mappings do not create ambiguous local/remote translations.
func ValidateMappings(mappings []Mapping) error {
	type normalizedMapping struct {
		local  string
		remote string
	}

	normalized := make([]normalizedMapping, len(mappings))
	for i, mapping := range mappings {
		local, err := normalizeLocalMappingPath(mapping.Local)
		if err != nil {
			return fmt.Errorf("mapping %d local path %q: %w", i+1, mapping.Local, err)
		}
		remote, err := normalizeRemoteMappingPath(mapping.Remote)
		if err != nil {
			return fmt.Errorf("mapping %d remote path %q: %w", i+1, mapping.Remote, err)
		}
		normalized[i] = normalizedMapping{local: local, remote: remote}
	}

	for i := range normalized {
		for j := i + 1; j < len(normalized); j++ {
			left := normalized[i]
			right := normalized[j]

			if left.local == right.local {
				return fmt.Errorf(
					"mappings %d and %d use the same local path %q",
					i+1, j+1, left.local,
				)
			}
			if left.remote == right.remote {
				return fmt.Errorf(
					"mappings %d and %d use the same remote path %q",
					i+1, j+1, left.remote,
				)
			}

			localForward, localForwardOK := descendantSuffix(left.local, right.local)
			remoteForward, remoteForwardOK := descendantSuffix(left.remote, right.remote)
			localReverse, localReverseOK := descendantSuffix(right.local, left.local)
			remoteReverse, remoteReverseOK := descendantSuffix(right.remote, left.remote)

			switch {
			case localForwardOK:
				if !remoteForwardOK || localForward != remoteForward {
					return ambiguousMappingsError(i, j)
				}
			case localReverseOK:
				if !remoteReverseOK || localReverse != remoteReverse {
					return ambiguousMappingsError(i, j)
				}
			case remoteForwardOK || remoteReverseOK:
				return ambiguousMappingsError(i, j)
			}
		}
	}

	return nil
}

func normalizeLocalMappingPath(value string) (string, error) {
	if strings.TrimSpace(value) == "" {
		return "", fmt.Errorf("must not be empty")
	}

	localPath := filepath.FromSlash(value)
	if filepath.IsAbs(localPath) {
		return "", fmt.Errorf("must be relative to the project root")
	}
	for _, segment := range strings.Split(filepath.ToSlash(localPath), "/") {
		if segment == ".." {
			return "", fmt.Errorf(`must not contain ".." segments`)
		}
	}
	localPath = filepath.Clean(localPath)
	if localPath == ".." || strings.HasPrefix(localPath, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("must not leave the project root")
	}
	return filepath.ToSlash(localPath), nil
}

func normalizeRemoteMappingPath(value string) (string, error) {
	if strings.TrimSpace(value) == "" {
		return "", fmt.Errorf("must not be empty")
	}
	if path.IsAbs(value) {
		return "", fmt.Errorf("must be relative to the host root")
	}
	for _, segment := range strings.Split(value, "/") {
		if segment == ".." {
			return "", fmt.Errorf(`must not contain ".." segments`)
		}
	}

	remotePath := path.Clean(value)
	if remotePath == ".." || strings.HasPrefix(remotePath, "../") {
		return "", fmt.Errorf("must not leave the host root")
	}
	return remotePath, nil
}

// descendantSuffix reports whether candidate is a strict descendant of base.
func descendantSuffix(base, candidate string) (string, bool) {
	if base == "." {
		if candidate == "." {
			return "", false
		}
		return candidate, true
	}
	prefix := base + "/"
	if !strings.HasPrefix(candidate, prefix) {
		return "", false
	}
	return strings.TrimPrefix(candidate, prefix), true
}

func ambiguousMappingsError(left, right int) error {
	return fmt.Errorf(
		"mappings %d and %d overlap differently on the local and remote sides",
		left+1, right+1,
	)
}
