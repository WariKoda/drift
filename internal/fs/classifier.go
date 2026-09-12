package fs

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
)

// PathClass describes properties that affect browser visibility and sync scope.
type PathClass struct {
	Hidden       bool
	Ignored      bool
	HardExcluded bool
	Reason       string
	RuleSource   string
	RuleLine     int
	RulePattern  string
}

// ClassifyCandidate describes a local project path. The path may represent a
// remote-only entry and does not need to exist locally.
type ClassifyCandidate struct {
	Path  string
	IsDir bool
}

type classKey struct {
	path  string
	isDir bool
}

// Classifier uses Git itself for ignore semantics and caches an immutable result
// for every path it has seen. Calls are batched by scan, never by ignore pattern.
type Classifier struct {
	projectRoot string
	repoRoot    string

	mu    sync.Mutex
	cache map[classKey]PathClass
}

// NewClassifier prepares a classifier and snapshots every existing project path.
// Git is also used outside repositories, with a temporary bare metadata directory
// that never writes into the project.
func NewClassifier(projectRoot string) (*Classifier, error) {
	root, err := filepath.Abs(projectRoot)
	if err != nil {
		return nil, err
	}
	root = filepath.Clean(root)
	if _, err := exec.LookPath("git"); err != nil {
		return nil, fmt.Errorf("Git is required to evaluate ignore rules: %w", err)
	}
	classifier := &Classifier{
		projectRoot: root,
		cache:       map[classKey]PathClass{},
	}

	cmd := exec.Command("git", "-C", root, "rev-parse", "--show-toplevel")
	output, revErr := cmd.Output()
	if revErr == nil {
		classifier.repoRoot = filepath.Clean(strings.TrimSpace(string(output)))
	} else if !isNotRepository(revErr) {
		return nil, fmt.Errorf("find Git worktree: %w", revErr)
	}

	candidates, err := existingCandidates(root)
	if err != nil {
		return nil, err
	}
	if _, err := classifier.ClassifyBatch(context.Background(), candidates); err != nil {
		return nil, err
	}
	return classifier, nil
}

func isNotRepository(err error) bool {
	var exitErr *exec.ExitError
	return errors.As(err, &exitErr) && exitErr.ExitCode() == 128 &&
		strings.Contains(strings.ToLower(string(exitErr.Stderr)), "not a git repository")
}

func existingCandidates(root string) ([]ClassifyCandidate, error) {
	var candidates []ClassifyCandidate
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if path == root {
			return nil
		}
		candidate := ClassifyCandidate{Path: path, IsDir: entry.IsDir()}
		class, classErr := lexicalClass(root, candidate)
		if classErr != nil {
			return classErr
		}
		candidates = append(candidates, candidate)
		if entry.IsDir() && class.HardExcluded {
			return filepath.SkipDir
		}
		return nil
	})
	return candidates, err
}

// ClassifyBatch evaluates candidates with one Git process for all cache misses.
func (c *Classifier) ClassifyBatch(ctx context.Context, candidates []ClassifyCandidate) ([]PathClass, error) {
	if c == nil {
		return nil, errors.New("path classifier is not initialized")
	}
	c.mu.Lock()
	defer c.mu.Unlock()

	results := make([]PathClass, len(candidates))
	unknownIndexes := make([]int, 0, len(candidates))
	queries := make([]string, 0, len(candidates))
	keys := make([]classKey, len(candidates))
	for i, candidate := range candidates {
		abs, err := filepath.Abs(candidate.Path)
		if err != nil {
			return nil, err
		}
		abs = filepath.Clean(abs)
		key := classKey{path: abs, isDir: candidate.IsDir}
		keys[i] = key
		if cached, ok := c.cache[key]; ok {
			results[i] = cached
			continue
		}
		class, err := lexicalClass(c.projectRoot, ClassifyCandidate{Path: abs, IsDir: candidate.IsDir})
		if err != nil {
			return nil, err
		}
		results[i] = class
		matchRoot := c.repoRoot
		if matchRoot == "" {
			matchRoot = c.projectRoot
		}
		relative, ok := relativeWithin(matchRoot, abs)
		if !ok {
			return nil, fmt.Errorf("path %q is outside Git worktree %q", abs, matchRoot)
		}
		query := filepath.ToSlash(relative)
		if candidate.IsDir && !strings.HasSuffix(query, "/") {
			query += "/"
		}
		if strings.IndexByte(query, 0) >= 0 {
			return nil, fmt.Errorf("path %q contains NUL", abs)
		}
		unknownIndexes = append(unknownIndexes, i)
		queries = append(queries, query)
	}
	if len(unknownIndexes) == 0 {
		return results, nil
	}

	args := []string{}
	temporaryGitDir := ""
	if c.repoRoot != "" {
		args = append(args, "-C", c.repoRoot)
	} else {
		gitDir, tempErr := os.MkdirTemp("", "drift-ignore-*")
		if tempErr != nil {
			return nil, fmt.Errorf("create temporary Git metadata: %w", tempErr)
		}
		temporaryGitDir = gitDir
		initCmd := exec.CommandContext(ctx, "git", "init", "--bare", "--quiet", gitDir)
		if initOutput, initErr := initCmd.CombinedOutput(); initErr != nil {
			removeErr := os.RemoveAll(gitDir)
			return nil, errors.Join(fmt.Errorf("initialize temporary Git metadata: %w: %s", initErr, strings.TrimSpace(string(initOutput))), removeErr)
		}
		args = append(args, "--git-dir="+gitDir, "--work-tree="+c.projectRoot, "-c", "core.excludesFile=/dev/null")
	}
	args = append(args, "check-ignore")
	if c.repoRoot == "" {
		args = append(args, "--no-index")
	}
	args = append(args, "--stdin", "-z", "--verbose", "--non-matching")
	cmd := exec.CommandContext(ctx, "git", args...)
	var input bytes.Buffer
	for _, query := range queries {
		input.WriteString(query)
		input.WriteByte(0)
	}
	cmd.Stdin = &input
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	if temporaryGitDir != "" {
		if removeErr := os.RemoveAll(temporaryGitDir); removeErr != nil {
			return nil, fmt.Errorf("remove temporary Git metadata: %w", removeErr)
		}
	}
	if err != nil {
		var exitErr *exec.ExitError
		if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
			if ctxErr := ctx.Err(); ctxErr != nil {
				return nil, ctxErr
			}
			return nil, fmt.Errorf("evaluate Git ignore rules: %w: %s", err, strings.TrimSpace(stderr.String()))
		}
	}

	fields := bytes.Split(stdout.Bytes(), []byte{0})
	if len(fields) != 4*len(unknownIndexes)+1 || len(fields) == 0 || len(fields[len(fields)-1]) != 0 {
		return nil, fmt.Errorf("evaluate Git ignore rules: malformed output")
	}
	for record, resultIndex := range unknownIndexes {
		source := string(fields[record*4])
		lineText := string(fields[record*4+1])
		pattern := string(fields[record*4+2])
		class := results[resultIndex]
		if source != "" {
			line, parseErr := strconv.Atoi(lineText)
			if parseErr != nil {
				return nil, fmt.Errorf("evaluate Git ignore rules: invalid line %q", lineText)
			}
			class.RuleSource = source
			class.RuleLine = line
			class.RulePattern = pattern
			class.Ignored = !strings.HasPrefix(pattern, "!")
			class.Reason = fmt.Sprintf("%s:%d:%s", source, line, pattern)
		}
		results[resultIndex] = class
		c.cache[keys[resultIndex]] = class
	}
	return results, nil
}

// CachedClassify returns a result without filesystem or Git work.
func (c *Classifier) CachedClassify(localPath string, isDir bool) (PathClass, bool) {
	if c == nil {
		return PathClass{}, false
	}
	abs, err := filepath.Abs(localPath)
	if err != nil {
		return PathClass{}, false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	class, ok := c.cache[classKey{path: filepath.Clean(abs), isDir: isDir}]
	return class, ok
}

func lexicalClass(projectRoot string, candidate ClassifyCandidate) (PathClass, error) {
	abs, err := filepath.Abs(candidate.Path)
	if err != nil {
		return PathClass{}, err
	}
	projectRel, ok := relativeWithin(projectRoot, abs)
	if !ok {
		return PathClass{}, fmt.Errorf("path %q is outside project %q", abs, projectRoot)
	}
	parts := splitPath(projectRel)
	class := PathClass{Hidden: hasHiddenPart(parts)}
	for i, part := range parts {
		terminalDirectory := i == len(parts)-1 && candidate.IsDir
		ancestorDirectory := i < len(parts)-1
		metadataFile := part == ".git" || part == ".svn" || part == ".hg"
		if ShouldSkipDir(part) && (terminalDirectory || ancestorDirectory || metadataFile) {
			class.HardExcluded = true
			class.Reason = part
			return class, nil
		}
	}
	return class, nil
}

func relativeWithin(root, candidate string) (string, bool) {
	rel, err := filepath.Rel(root, filepath.Clean(candidate))
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", false
	}
	return rel, true
}

func splitPath(path string) []string {
	path = filepath.ToSlash(filepath.Clean(path))
	if path == "." || path == "" {
		return nil
	}
	return strings.Split(path, "/")
}

// IsHiddenPath reports whether any component of a relative display path starts with a dot.
func IsHiddenPath(relative string) bool {
	return hasHiddenPart(splitPath(relative))
}

func hasHiddenPart(parts []string) bool {
	for _, part := range parts {
		if strings.HasPrefix(part, ".") && part != "." && part != ".." {
			return true
		}
	}
	return false
}
