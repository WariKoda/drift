package cmd

import (
	"fmt"
	"os"

	"github.com/WariKoda/drift/internal/tui"
	"github.com/spf13/cobra"
)

var dashCmd = &cobra.Command{
	Use:   "dash",
	Short: "Open the project dashboard",
	Long:  "dash opens the project dashboard regardless of the current directory.",
	RunE: func(cmd *cobra.Command, args []string) error {
		workDir, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("cannot determine working directory: %w", err)
		}
		cfg, store, reg, err := loadAll(workDir)
		if err != nil {
			return err
		}
		app, err := tui.New(workDir, cfg, store, reg, tui.ScreenDashboard)
		if err != nil {
			return err
		}
		return runProgram(app)
	},
}

func init() {
	rootCmd.AddCommand(dashCmd)
}
