package cmd

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"
)

var (
	flagDashboard   bool
	flagNoDashboard bool
	flagLog         string
	flagDebug       bool
	flagNoMouse     bool
)

var rootCmd = &cobra.Command{
	Use:           "drift",
	Short:         "Terminal remote file sync — browse, diff, and sync with remote hosts",
	SilenceUsage:  true,
	SilenceErrors: true,
	Long: `drift opens a file browser in the current directory.
Select files or folders, then sync them with a configured remote host over SFTP/SSH.

When started outside a registered project, drift reopens the last project
you opened, if that path still exists. Otherwise it shows the dashboard when
projects are registered. --dashboard forces the list; --no-dashboard stays in
the current directory.

Config locations (nothing is written into your project):
  global:   ~/.config/drift/config.toml
  project:  ~/.config/drift/projects/<slug>.toml
  registry: ~/.config/drift/projects.toml`,
	RunE: func(cmd *cobra.Command, args []string) error {
		workDir, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("cannot determine working directory: %w", err)
		}

		cfg, root, store, reg, err := loadAll(workDir)
		if err != nil {
			return err
		}

		last := usableLastProject(reg)
		mode := resolveStart(flagDashboard, flagNoDashboard, worthStayingIn(workDir, cfg, reg), last, len(reg.Projects))

		initial := tui.ScreenBrowser
		switch mode {
		case startDashboard:
			initial = tui.ScreenDashboard
		case startLastProject:
			root = last.Path
			cfg, err = config.Load(last.Path, last.Slug)
			if err != nil {
				return fmt.Errorf("config error: %w", err)
			}
		}

		app, err := tui.New(root, cfg, store, reg, initial)
		if err != nil {
			return fmt.Errorf("cannot read directory: %w", err)
		}
		return runProgram(app, resolveMouseEnabled(cfg))
	},
}

func init() {
	rootCmd.Flags().BoolVar(&flagDashboard, "dashboard", false, "always start on the project dashboard")
	rootCmd.Flags().BoolVar(&flagNoDashboard, "no-dashboard", false, "never start on the project dashboard")
	rootCmd.PersistentFlags().StringVar(&flagLog, "log", "", "write diagnostics to this log file (overrides $DRIFT_LOG)")
	rootCmd.PersistentFlags().BoolVar(&flagDebug, "debug", false, "enable debug logging (default file: <config dir>/drift.log)")
	rootCmd.PersistentFlags().BoolVar(&flagNoMouse, "no-mouse", false, "disable mouse reporting (restores the terminal's own text selection)")
}

// resolveMouseEnabled decides whether to turn on mouse reporting.
// Precedence: --no-mouse flag > $DRIFT_NO_MOUSE > config [ui].mouse > enabled.
func resolveMouseEnabled(cfg *config.MergedConfig) bool {
	if flagNoMouse || envTruthy(os.Getenv("DRIFT_NO_MOUSE")) {
		return false
	}
	return cfg.MouseEnabled()
}

// resolveLogConfig derives logging options from flags and environment.
// Flags win over environment. Logging is enabled when a path is given or debug
// is on; with debug but no explicit path it defaults to <config dir>/drift.log.
func resolveLogConfig() (opts log.Options, enabled bool) {
	path := flagLog
	if path == "" {
		path = os.Getenv("DRIFT_LOG")
	}
	debug := flagDebug || envTruthy(os.Getenv("DRIFT_DEBUG"))

	enabled = path != "" || debug
	if !enabled {
		return log.Options{}, false
	}
	if path == "" {
		path = filepath.Join(config.Dir(), "drift.log")
	}
	return log.Options{Path: path, Debug: debug}, true
}

// envTruthy reports whether an environment value means "on". Empty/0/false are off.
func envTruthy(v string) bool {
	if v == "" {
		return false
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		return true // any non-empty, non-bool value (e.g. a path) counts as set
	}
	return b
}

// startMode is how `drift` with no subcommand chooses its first screen.
type startMode int

const (
	startBrowserCwd startMode = iota
	startDashboard
	startLastProject
)

// worthStayingIn reports whether the working directory is a place drift should
// open the browser in rather than jumping to the last project.
//
// It belongs to a registered project, or it is one drift can offer to register:
// a repository, or a directory with a leftover .drift/config.toml that still
// needs migrating. Jumping away from either would hide the offer.
func worthStayingIn(workDir string, cfg *config.MergedConfig, reg *project.Registry) bool {
	if cfg.ProjectSlug != "" {
		return true
	}
	if root, ok := config.FindLegacyProjectRoot(workDir); ok && !reg.HasPath(root) {
		return true
	}
	root, ok := project.GitRoot(workDir)
	return ok && !reg.HasPath(root)
}

// resolveStart decides the no-subcommand start screen.
// --no-dashboard wins; --dashboard forces the list; inside a project the
// browser stays in cwd; outside, a usable last-opened project is restored.
func resolveStart(dashFlag, noDashFlag, hasProjectCtx bool, last *project.Project, regCount int) startMode {
	if noDashFlag {
		return startBrowserCwd
	}
	if dashFlag {
		return startDashboard
	}
	if hasProjectCtx {
		return startBrowserCwd
	}
	if last != nil {
		return startLastProject
	}
	if regCount > 0 {
		return startDashboard
	}
	return startBrowserCwd
}

// usableLastProject returns MostRecentlyOpened if its path exists and is a directory.
func usableLastProject(reg *project.Registry) *project.Project {
	p := reg.MostRecentlyOpened()
	if p == nil {
		return nil
	}
	info, err := os.Stat(p.Path)
	if err != nil || !info.IsDir() {
		return nil
	}
	return p
}

// loadAll loads the merged config and the project registry for workDir.
func loadAll(workDir string) (*config.MergedConfig, string, *project.Store, *project.Registry, error) {
	store := project.NewStore()
	reg, err := store.Load()
	if err != nil {
		return nil, "", nil, nil, fmt.Errorf("cannot read project registry (%s): %w", store.Path(), err)
	}

	// The registry decides which project a directory belongs to; there is no
	// marker file in the project any more.
	root, slug := workDir, ""
	if p := reg.FindByPathPrefix(workDir); p != nil {
		root, slug = p.Path, p.Slug
	}

	cfg, err := config.Load(root, slug)
	if err != nil {
		return nil, "", nil, nil, fmt.Errorf("config error: %w", err)
	}
	return cfg, root, store, reg, nil
}

func runProgram(app tui.App, mouse bool) error {
	if opts, enabled := resolveLogConfig(); enabled {
		closer, err := log.Init(opts)
		if err != nil {
			// Warn before the alt screen takes over; continue without logging.
			fmt.Fprintf(os.Stderr, "warning: could not open log file %s: %v\n", opts.Path, err)
		} else {
			defer closer.Close()
			log.Info("drift start", "version", resolvedVersion())
		}
	}

	opts := []tea.ProgramOption{tea.WithAltScreen()}
	if mouse {
		// CellMotion, not AllMotion: clicks and wheel plus drag, but no event
		// on every idle cursor move. Nothing here reacts to plain hover.
		opts = append(opts, tea.WithMouseCellMotion())
	}

	p := tea.NewProgram(app, opts...)
	if _, err := p.Run(); err != nil {
		return fmt.Errorf("TUI error: %w", err)
	}
	return nil
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
