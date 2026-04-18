package cmd

import (
	"fmt"
	"runtime/debug"
	"strings"

	"github.com/spf13/cobra"
)

// Version is injected at build time via -ldflags "-X github.com/WariKoda/drift/cmd.Version=x.y.z"
var Version = "dev"

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print drift version",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("drift %s\n", resolvedVersion())
	},
}

func init() {
	rootCmd.AddCommand(versionCmd)
}

func resolvedVersion() string {
	if Version != "" && Version != "dev" {
		return Version
	}

	info, ok := debug.ReadBuildInfo()
	if !ok {
		return Version
	}

	if info.Main.Version != "" && info.Main.Version != "(devel)" {
		return info.Main.Version
	}

	for _, setting := range info.Settings {
		switch setting.Key {
		case "vcs.revision":
			if len(setting.Value) >= 7 {
				return "dev-" + setting.Value[:7]
			}
			if setting.Value != "" {
				return "dev-" + setting.Value
			}
		case "vcs.modified":
			if strings.EqualFold(setting.Value, "true") {
				return "dev-dirty"
			}
		}
	}

	return Version
}
