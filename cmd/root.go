package cmd

import (
	"fmt"
	"os"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tui"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"
)

var rootCmd = &cobra.Command{
	Use:   "drift",
	Short: "Terminal remote file sync — browse, diff, and sync with remote hosts",
	Long: `drift opens a file browser in the current directory.
Select files or folders, then sync them with a configured remote host over SFTP/SSH.

Config locations:
  global:  ~/.config/drift/config.toml
  project: .drift/config.toml  (in current or any parent directory)`,
	RunE: func(cmd *cobra.Command, args []string) error {
		workDir, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("cannot determine working directory: %w", err)
		}

		cfg, err := config.Load(workDir)
		if err != nil {
			return fmt.Errorf("config error: %w", err)
		}

		app, err := tui.New(workDir, cfg)
		if err != nil {
			return fmt.Errorf("cannot read directory: %w", err)
		}

		p := tea.NewProgram(app,
			tea.WithAltScreen(),
		)
		if _, err := p.Run(); err != nil {
			return fmt.Errorf("TUI error: %w", err)
		}
		return nil
	},
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
