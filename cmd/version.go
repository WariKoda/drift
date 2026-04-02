package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
)

// Version is injected at build time via -ldflags "-X github.com/yourusername/drift/cmd.Version=x.y.z"
var Version = "dev"

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print drift version",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("drift %s\n", Version)
	},
}

func init() {
	rootCmd.AddCommand(versionCmd)
}
