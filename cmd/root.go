package cmd

import (
	"fmt"
	"os"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"
)

var (
	flagDashboard   bool
	flagNoDashboard bool
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
