package tui

import (
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/fs"
	internalsync "github.com/WariKoda/drift/internal/sync"
)

// Screen represents which TUI screen is currently active.
type Screen int

const (
	ScreenBrowser      Screen = iota
	ScreenHostSelector        // modal overlay on browser (sync target picker)
	ScreenHostManager         // CRUD list of all hosts
	ScreenHostForm            // create / edit a host
	ScreenDiffLoading         // async SSH connect + diff load in progress
	ScreenDiffView            // split-pane diff
	ScreenSyncProgress        // transfer progress (Phase 4)
)

// StatusKind classifies the severity of a status bar message.
type StatusKind int

const (
	StatusInfo StatusKind = iota
	StatusWarn
	StatusError
)

// AppState is the root state of the application.
type AppState struct {
	Screen     Screen
	WorkingDir string
	Config     *config.MergedConfig

	// Browser
	Selection *fs.SelectionState

	// Host selector (sync modal)
	SelectedHost *config.Host

	// Diff (Phase 3)
	DiffSessions  []diff.Session
	ActiveSession int

	// Sync (Phase 4)
	SyncPlan     *internalsync.Plan
	SyncProgress *internalsync.Progress

	// Status bar
	StatusMsg  string
	StatusKind StatusKind

	TermWidth  int
	TermHeight int
}
