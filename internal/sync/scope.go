// Package sync decides sync directions and describes comparison scope.
package sync

// ScopeOptions controls which recursively discovered files enter a comparison.
type ScopeOptions struct {
	IncludeIgnored bool
}

// ScopeSummary explains how selections were reduced to comparable file pairs.
type ScopeSummary struct {
	Pairs                   int
	Hidden                  int
	IgnoredFilesSkipped     int
	IgnoredDirsSkipped      int
	HardExcludedSkipped     int
	ExplicitIgnoredIncluded int
}
