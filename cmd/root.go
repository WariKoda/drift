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
)

var rootCmd = &cobra.Command{
	Use:           "drift",
	Short:         "Terminal remote file sync — browse, diff, and sync with remote hosts",
	SilenceUsage:  true,
	SilenceErrors: true,
	Long: `drift opens a file browser in the current directory.
Select files or folders, then sync them with a configured remote host over SFTP/SSH.

When started outside a project (no .drift/ found) and projects are registered,
drift shows a dashboard of your projects instead. Use --dashboard or --no-dashboard
to force either behaviour.

Config locations:
  global:   ~/.config/drift/config.toml
  project:  .drift/config.toml  (in current or any parent directory)
  registry: ~/.config/drift/projects.toml`,
	RunE: func(cmd *cobra.Command, args []string) error {
		workDir, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("cannot determine working directory: %w", err)
		}

		cfg, store, reg, err := loadAll(workDir)
		if err != nil {
			return err
		}

		initial := tui.ScreenBrowser
		if shouldShowDashboard(flagDashboard, flagNoDashboard, config.HasProjectContext(workDir), len(reg.Projects)) {
			initial = tui.ScreenDashboard
		}

		app, err := tui.New(workDir, cfg, store, reg, initial)
		if err != nil {
			return fmt.Errorf("cannot read directory: %w", err)
		}
		return runProgram(app)
	},
}

func init() {
	rootCmd.Flags().BoolVar(&flagDashboard, "dashboard", false, "always start on the project dashboard")
	rootCmd.Flags().BoolVar(&flagNoDashboard, "no-dashboard", false, "never start on the project dashboard")
	rootCmd.PersistentFlags().StringVar(&flagLog, "log", "", "write diagnostics to this log file (overrides $DRIFT_LOG)")
	rootCmd.PersistentFlags().BoolVar(&flagDebug, "debug", false, "enable debug logging (default file: <config dir>/drift.log)")
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

// shouldShowDashboard decides whether the dashboard is the start screen.
// Explicit flags win; otherwise the dashboard appears only when drift is run
// outside a project context and at least one project is registered.
func shouldShowDashboard(dashFlag, noDashFlag, hasProjectCtx bool, regCount int) bool {
	if noDashFlag {
		return false
	}
	if dashFlag {
		return true
	}
	return !hasProjectCtx && regCount > 0
}

// loadAll loads the merged config and the project registry for workDir.
func loadAll(workDir string) (*config.MergedConfig, *project.Store, *project.Registry, error) {
	cfg, err := config.Load(workDir)
	if err != nil {
		return nil, nil, nil, fmt.Errorf("config error: %w", err)
	}
	store := project.NewStore()
	reg, err := store.Load()
	if err != nil {
		return nil, nil, nil, fmt.Errorf("cannot read project registry (%s): %w", store.Path(), err)
	}
	return cfg, store, reg, nil
}

func runProgram(app tui.App) error {
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

	p := tea.NewProgram(app, tea.WithAltScreen())
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
