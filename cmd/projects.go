package cmd

import (
	"fmt"
	"os"
	"text/tabwriter"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui"
	"github.com/spf13/cobra"
)

var (
	flagEditName string
	flagEditPath string
)

var projectsCmd = &cobra.Command{
	Use:   "projects",
	Short: "Manage the project registry",
	Long:  "List, add, edit, remove and archive registered drift projects.",
}

var projectsListCmd = &cobra.Command{
	Use:   "list",
	Short: "List registered projects",
	RunE: func(cmd *cobra.Command, args []string) error {
		reg, err := loadRegistry()
		if err != nil {
			return err
		}
		projects := reg.All()
		if len(projects) == 0 {
			fmt.Println("No projects registered. Add one with: drift projects add <name> [path]")
			return nil
		}
		w := tabwriter.NewWriter(os.Stdout, 0, 4, 2, ' ', 0)
		fmt.Fprintln(w, "SLUG\tNAME\tPATH\tSTATUS")
		for _, p := range projects {
			fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", p.Slug, p.Name, p.Path, projectStatus(p))
		}
		return w.Flush()
	},
}

var projectsAddCmd = &cobra.Command{
	Use:   "add <name> [path]",
	Short: "Register a new project",
	Args:  cobra.RangeArgs(1, 2),
	RunE: func(cmd *cobra.Command, args []string) error {
		name := args[0]
		rawPath := "."
		if len(args) == 2 {
			rawPath = args[1]
		}
		path, err := project.ExpandPath(rawPath)
		if err != nil {
			return err
		}

		store := project.NewStore()
		reg, err := store.Load()
		if err != nil {
			return registryError(store, err)
		}
		slug := reg.UniqueSlug(project.Slugify(name))
		now := time.Now().UTC()
		if err := reg.Add(project.Project{
			Slug: slug, Name: name, Path: path, CreatedAt: now, UpdatedAt: now,
		}); err != nil {
			return err
		}
		if err := store.Save(reg); err != nil {
			return err
		}
		fmt.Printf("Added project %q (%s) → %s\n", name, slug, path)
		return nil
	},
}

var projectsEditCmd = &cobra.Command{
	Use:   "edit <slug>",
	Short: "Edit a registered project",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		slug := args[0]
		store := project.NewStore()
		reg, err := store.Load()
		if err != nil {
			return registryError(store, err)
		}
		p := reg.Find(slug)
		if p == nil {
			return fmt.Errorf("no project with slug %q", slug)
		}
		updated := *p
		if flagEditName != "" {
			updated.Name = flagEditName
		}
		if flagEditPath != "" {
			abs, err := project.ExpandPath(flagEditPath)
			if err != nil {
				return err
			}
			updated.Path = abs
		}
		updated.UpdatedAt = time.Now().UTC()
		if err := reg.Update(slug, updated); err != nil {
			return err
		}
		if err := store.Save(reg); err != nil {
			return err
		}
		fmt.Printf("Updated project %q\n", slug)
		return nil
	},
}

var projectsRemoveCmd = &cobra.Command{
	Use:   "remove <slug>",
	Short: "Remove a project from the registry",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		slug := args[0]
		store := project.NewStore()
		reg, err := store.Load()
		if err != nil {
			return registryError(store, err)
		}
		if err := reg.Remove(slug); err != nil {
			return err
		}
		if err := store.Save(reg); err != nil {
			return err
		}
		fmt.Printf("Removed project %q\n", slug)
		return nil
	},
}

var projectsArchiveCmd = &cobra.Command{
	Use:   "archive <slug>",
	Short: "Toggle a project's archived state",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		slug := args[0]
		store := project.NewStore()
		reg, err := store.Load()
		if err != nil {
			return registryError(store, err)
		}
		p := reg.Find(slug)
		if p == nil {
			return fmt.Errorf("no project with slug %q", slug)
		}
		updated := *p
		updated.Archived = !updated.Archived
		updated.UpdatedAt = time.Now().UTC()
		if err := reg.Update(slug, updated); err != nil {
			return err
		}
		if err := store.Save(reg); err != nil {
			return err
		}
		state := "archived"
		if !updated.Archived {
			state = "active"
		}
		fmt.Printf("Project %q is now %s\n", slug, state)
		return nil
	},
}

var openCmd = &cobra.Command{
	Use:   "open <slug>",
	Short: "Open a registered project directly",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		slug := args[0]
		store := project.NewStore()
		reg, err := store.Load()
		if err != nil {
			return registryError(store, err)
		}
		p := reg.Find(slug)
		if p == nil {
			return fmt.Errorf("no project with slug %q", slug)
		}
		if info, err := os.Stat(p.Path); err != nil || !info.IsDir() {
			return fmt.Errorf("project path does not exist: %s", p.Path)
		}
		cfg, err := config.Load(p.Path)
		if err != nil {
			return fmt.Errorf("config error: %w", err)
		}
		app, err := tui.New(p.Path, cfg, store, reg, tui.ScreenBrowser)
		if err != nil {
			return fmt.Errorf("cannot read directory: %w", err)
		}
		return runProgram(app)
	},
}

func init() {
	projectsEditCmd.Flags().StringVar(&flagEditName, "name", "", "new display name")
	projectsEditCmd.Flags().StringVar(&flagEditPath, "path", "", "new local path")

	projectsCmd.AddCommand(projectsListCmd, projectsAddCmd, projectsEditCmd, projectsRemoveCmd, projectsArchiveCmd)
	rootCmd.AddCommand(projectsCmd, openCmd)
}

func loadRegistry() (*project.Registry, error) {
	store := project.NewStore()
	reg, err := store.Load()
	if err != nil {
		return nil, registryError(store, err)
	}
	return reg, nil
}

func registryError(store *project.Store, err error) error {
	return fmt.Errorf("cannot read project registry (%s): %w", store.Path(), err)
}

func projectStatus(p project.Project) string {
	if info, err := os.Stat(p.Path); err != nil || !info.IsDir() {
		return "missing"
	}
	if p.Archived {
		return "archived"
	}
	return "active"
}
