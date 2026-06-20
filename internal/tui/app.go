package tui

import (
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/dashboard"
	"github.com/WariKoda/drift/internal/tui/diffview"
	"github.com/WariKoda/drift/internal/tui/hostform"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
	"github.com/WariKoda/drift/internal/tui/hostselector"
	"github.com/WariKoda/drift/internal/tui/projectform"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// App is the root bubbletea Model.
type App struct {
	state            AppState
	browser          browser.Model
	hostManager      hostmanager.Model
	hostForm         hostform.Model
	hostSel          hostselector.Model
	diffView         diffview.Model
	diffLoadProgress diffview.LoadProgress
	diffLoadTracker  *diffview.LoadProgressTracker
	diffLoadPulse    int
	dashboard        dashboard.Model
	projectForm      projectform.Model

	// Project registry (nil when drift was launched without dashboard support).
	store    *project.Store
	registry *project.Registry
}

// New creates a fully initialised App. When initial is ScreenDashboard the app
// starts on the project dashboard; otherwise it opens the file browser in
// workDir (the classic behaviour). store and reg may be nil when the registry
// is unavailable — the dashboard is then simply unreachable.
func New(workDir string, cfg *config.MergedConfig, store *project.Store, reg *project.Registry, initial Screen) (App, error) {
	a := App{
		state: AppState{
			Screen:     initial,
			WorkingDir: workDir,
			Config:     cfg,
		},
		store:    store,
		registry: reg,
	}

	if initial == ScreenDashboard {
		a.dashboard = dashboard.New(reg, 0, 0)
		return a, nil
	}

	b, err := browser.New(workDir)
	if err != nil {
		return App{}, err
	}
	a.browser = b
	a.state.Selection = b.Selection
	a.state.RemoteSelection = b.RemoteSelection

	// Offer to register the current project if it isn't in the registry yet.
	if shouldPromptRegister(workDir, cfg, reg) {
		a.state.Screen = ScreenRegisterPrompt
		a.state.PendingRegisterPath = cfg.ProjectRoot
		a.state.PendingRegisterName = filepath.Base(cfg.ProjectRoot)
	}
	return a, nil
}

// shouldPromptRegister reports whether drift should offer to register the
// current directory: it is inside a real .drift project that has no matching
// registry entry yet.
func shouldPromptRegister(workDir string, cfg *config.MergedConfig, reg *project.Registry) bool {
	if cfg == nil || reg == nil {
		return false
	}
	if !config.HasProjectContext(workDir) {
		return false
	}
	return cfg.ProjectRoot != "" && !reg.HasPath(cfg.ProjectRoot)
}

// registerPending adds the pending project to the registry and persists it.
func (a *App) registerPending() error {
	now := time.Now().UTC()
	slug := a.registry.UniqueSlug(project.Slugify(a.state.PendingRegisterName))
	p := project.Project{
		Slug:      slug,
		Name:      a.state.PendingRegisterName,
		Path:      a.state.PendingRegisterPath,
		CreatedAt: now,
		UpdatedAt: now,
	}
	if err := a.registry.Add(p); err != nil {
		return err
	}
	if err := a.store.Save(a.registry); err != nil {
		return err
	}
	pc := p
	a.state.ActiveProject = &pc
	return nil
}

func (a App) Init() tea.Cmd {
	if a.state.Screen == ScreenDashboard {
		return a.dashboard.Init()
	}
	return a.browser.Init()
}

// openProject re-roots the running app into p: it loads p's config, builds a
// fresh browser at p.Path and switches to the browser screen.
func (a *App) openProject(p project.Project) (tea.Cmd, error) {
	a.browser.CloseRemote()
	cfg, err := config.Load(p.Path)
	if err != nil {
		return nil, err
	}
	b, err := browser.New(p.Path)
	if err != nil {
		return nil, err
	}
	b.SetSize(a.state.TermWidth, a.state.TermHeight)

	a.browser = b
	a.state.Config = cfg
	a.state.WorkingDir = p.Path
	a.state.Selection = b.Selection
	a.state.RemoteSelection = b.RemoteSelection
	pc := p
	a.state.ActiveProject = &pc
	a.state.Screen = ScreenBrowser
	return b.Init(), nil
}

// saveProjectForm persists a created/edited project from the project form.
func (a *App) saveProjectForm(msg projectform.MsgProjectSaved) error {
	path, err := project.ExpandPath(msg.Path)
	if err != nil {
		return err
	}
	now := time.Now().UTC()

	if msg.OldSlug == "" {
		slug := a.registry.UniqueSlug(project.Slugify(msg.Name))
		return a.persist(func() error {
			return a.registry.Add(project.Project{
				Slug:      slug,
				Name:      msg.Name,
				Path:      path,
				CreatedAt: now,
				UpdatedAt: now,
			})
		})
	}

	existing := a.registry.Find(msg.OldSlug)
	if existing == nil {
		return fmt.Errorf("project %q no longer exists", msg.OldSlug)
	}
	updated := *existing
	updated.Name = msg.Name
	updated.Path = path
	updated.UpdatedAt = now
	return a.persist(func() error {
		return a.registry.Update(msg.OldSlug, updated)
	})
}

// persist runs a registry mutation and writes the registry to disk, refreshing
// the dashboard view on success.
func (a *App) persist(mutate func() error) error {
	if err := mutate(); err != nil {
		return err
	}
	if err := a.store.Save(a.registry); err != nil {
		return err
	}
	a.dashboard.Refresh(a.registry)
	return nil
}

func (a App) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	// ── Terminal resize ───────────────────────────────────────────────
	case tea.WindowSizeMsg:
		a.state.TermWidth = msg.Width
		a.state.TermHeight = msg.Height
		a.browser.SetSize(msg.Width, msg.Height)
		a.hostManager.SetSize(msg.Width, msg.Height)
		a.hostForm.SetSize(msg.Width, msg.Height)
		a.hostSel.Width = msg.Width
		a.hostSel.Height = msg.Height
		a.diffView.SetSize(msg.Width, msg.Height)
		a.dashboard.SetSize(msg.Width, msg.Height)
		a.projectForm.SetSize(msg.Width, msg.Height)
		return a, nil

	// ── Project Dashboard ─────────────────────────────────────────────
	case dashboard.MsgProjectChosen:
		cmd, err := a.openProject(msg.Project)
		if err != nil {
			a.dashboard.SetStatus("Cannot open project: " + err.Error())
			return a, nil
		}
		return a, cmd

	case dashboard.MsgOpenProjectForm:
		if msg.Project != nil {
			a.projectForm = projectform.NewEdit(*msg.Project, a.state.TermWidth, a.state.TermHeight)
		} else {
			// Pre-fill a new project with the current working directory.
			a.projectForm = projectform.New(
				filepath.Base(a.state.WorkingDir), a.state.WorkingDir,
				a.state.TermWidth, a.state.TermHeight)
		}
		a.state.Screen = ScreenProjectForm
		return a, nil

	case dashboard.MsgDeleteProject:
		if err := a.persist(func() error { return a.registry.Remove(msg.Slug) }); err != nil {
			a.dashboard.SetStatus("Delete failed: " + err.Error())
		}
		a.state.Screen = ScreenDashboard
		return a, nil

	case dashboard.MsgArchiveProject:
		if p := a.registry.Find(msg.Slug); p != nil {
			updated := *p
			updated.Archived = !updated.Archived
			updated.UpdatedAt = time.Now().UTC()
			if err := a.persist(func() error { return a.registry.Update(msg.Slug, updated) }); err != nil {
				a.dashboard.SetStatus("Archive failed: " + err.Error())
			}
		}
		a.state.Screen = ScreenDashboard
		return a, nil

	case dashboard.MsgDashboardQuit:
		return a, tea.Quit

	// ── Project Form ──────────────────────────────────────────────────
	case projectform.MsgProjectSaved:
		if err := a.saveProjectForm(msg); err != nil {
			a.projectForm.SetErr("Save failed: " + err.Error())
			a.state.Screen = ScreenProjectForm
			return a, nil
		}
		a.state.Screen = ScreenDashboard
		return a, nil

	case projectform.MsgProjectFormCancelled:
		a.state.Screen = ScreenDashboard
		return a, nil

	// ── Browser → Dashboard ───────────────────────────────────────────
	case browser.MsgOpenDashboard:
		a.browser.CloseRemote()
		if a.store == nil || a.registry == nil {
			return a, nil
		}
		if reg, err := a.store.Load(); err == nil {
			a.registry = reg
			a.dashboard.Refresh(reg)
		}
		a.state.Screen = ScreenDashboard
		return a, nil

	// ── Browser → Host Selector / direct sync ─────────────────────────
	case browser.MsgSyncRequested:
		a.state.Selection = msg.Selection
		a.state.RemoteSelection = msg.RemoteSelection
		if msg.Host != nil {
			h := *msg.Host
			tracker := diffview.NewLoadProgressTracker()
			a.diffLoadProgress, _ = tracker.Snapshot()
			a.diffLoadTracker = tracker
			a.diffLoadPulse = 0
			a.state.SelectedHost = &h
			a.state.Screen = ScreenDiffLoading
			return a, tea.Batch(
				diffview.LoadCmd(h, a.state.Selection, a.state.RemoteSelection, a.state.Config, msg.Conn, tracker),
				diffview.ProgressTickCmd(tracker),
			)
		}
		a.state.HostSelectorPurpose = HostSelectorForSync
		a.hostSel = hostselector.New(a.state.Config, a.state.TermWidth, a.state.TermHeight)
		a.state.Screen = ScreenHostSelector
		return a, nil

	case browser.MsgBrowseRemoteRequested:
		a.state.HostSelectorPurpose = HostSelectorForRemoteBrowse
		a.hostSel = hostselector.New(a.state.Config, a.state.TermWidth, a.state.TermHeight)
		a.state.Screen = ScreenHostSelector
		return a, nil

	// ── Host chosen → sync or load remote browser ─────────────────────
	case hostselector.MsgHostChosen:
		h := msg.Host
		if a.state.HostSelectorPurpose == HostSelectorForRemoteBrowse {
			a.state.Screen = ScreenBrowser
			cmd := a.browser.StartRemote(h)
			return a, cmd
		}
		tracker := diffview.NewLoadProgressTracker()
		a.diffLoadProgress, _ = tracker.Snapshot()
		a.diffLoadTracker = tracker
		a.diffLoadPulse = 0
		a.state.SelectedHost = &h
		a.state.Screen = ScreenDiffLoading
		return a, tea.Batch(
			diffview.LoadCmd(h, a.state.Selection, a.state.RemoteSelection, a.state.Config, nil, tracker),
			diffview.ProgressTickCmd(tracker),
		)

	case hostselector.MsgSelectorCancelled:
		a.state.Screen = ScreenBrowser
		return a, nil

	case browser.MsgRemoteLoaded:
		if a.state.Screen != ScreenBrowser {
			if msg.Conn != nil {
				_ = msg.Conn.Close()
			}
			return a, nil
		}
		var cmd tea.Cmd
		a.browser, cmd = a.browser.Update(msg)
		return a, cmd

	case browser.MsgRemoteChildrenLoaded:
		if a.state.Screen != ScreenBrowser {
			return a, nil
		}
		var cmd tea.Cmd
		a.browser, cmd = a.browser.Update(msg)
		return a, cmd

	// ── Diff loading progress / loaded ────────────────────────────────
	case diffview.MsgDiffLoadProgress:
		if a.state.Screen != ScreenDiffLoading || msg.Tracker != a.diffLoadTracker {
			return a, nil
		}
		a.diffLoadProgress = msg.Progress
		a.diffLoadPulse++
		if msg.Done {
			return a, nil
		}
		return a, diffview.ProgressTickCmd(msg.Tracker)

	case diffview.MsgDiffLoaded:
		if a.state.Screen != ScreenDiffLoading {
			if msg.Conn != nil {
				_ = msg.Conn.Close()
			}
			return a, nil
		}
		a.diffView = diffview.New(
			msg.Sessions,
			*a.state.SelectedHost,
			msg.Conn, // connection stays open for sync ops
			a.state.TermWidth,
			a.state.TermHeight,
		)
		a.state.Screen = ScreenDiffView
		return a, nil

	case diffview.MsgDiffError:
		if a.state.Screen != ScreenDiffLoading {
			return a, nil
		}
		a.state.Screen = ScreenBrowser
		a.browser.SetStatus("Connection failed: " + msg.Err.Error())
		return a, nil

	// ── Diff view → back to browser ───────────────────────────────────
	case diffview.MsgBackToBrowser:
		a.diffView.Close()
		a.state.Screen = ScreenBrowser
		a.state.Selection.Clear()
		if a.state.RemoteSelection != nil {
			a.state.RemoteSelection.Clear()
		}
		if a.state.SelectedHost != nil {
			h := *a.state.SelectedHost
			return a, a.browser.StartRemote(h)
		}
		return a, nil

	// ── Host Manager ──────────────────────────────────────────────────
	case browser.MsgOpenHostManager:
		a.hostManager = hostmanager.New(a.state.Config, a.state.TermWidth, a.state.TermHeight)
		a.state.Screen = ScreenHostManager
		return a, nil

	case hostmanager.MsgBackToBrowser:
		a.state.Screen = ScreenBrowser
		return a, nil

	case hostmanager.MsgOpenForm:
		if msg.Host != nil {
			a.hostForm = hostform.NewEdit(*msg.Host, msg.Scope,
				a.state.Config.ProjectRoot, a.state.TermWidth, a.state.TermHeight)
		} else {
			a.hostForm = hostform.New(msg.Scope,
				a.state.Config.ProjectRoot, a.state.TermWidth, a.state.TermHeight)
		}
		a.state.Screen = ScreenHostForm
		return a, nil

	case hostmanager.MsgDeleteHost:
		var err error
		if msg.Scope == config.ScopeGlobal {
			err = config.DeleteGlobalHost(a.state.Config, msg.Name)
		} else {
			err = config.DeleteProjectHost(a.state.Config, msg.Name)
		}
		if err != nil {
			a.state.StatusMsg = "Delete failed: " + err.Error()
		}
		a.hostManager.Refresh()
		a.state.Screen = ScreenHostManager
		return a, nil

	case hostform.MsgHostSaved:
		var err error
		if msg.Scope == config.ScopeGlobal {
			err = config.SaveGlobalHost(a.state.Config, msg.Host, msg.OldName)
		} else {
			err = config.SaveProjectHost(a.state.Config, msg.Host, msg.OldName)
		}
		if err != nil {
			a.hostForm.SetErr("Save failed: " + err.Error())
			a.state.Screen = ScreenHostForm
			return a, nil
		}
		a.hostManager.Refresh()
		a.state.Screen = ScreenHostManager
		return a, nil

	case hostform.MsgFormCancelled:
		a.state.Screen = ScreenHostManager
		return a, nil
	}

	// ── Delegate to active screen ─────────────────────────────────────
	switch a.state.Screen {
	case ScreenRegisterPrompt:
		key, ok := msg.(tea.KeyMsg)
		if !ok {
			return a, nil
		}
		switch key.String() {
		case "y", "Y", "enter":
			if err := a.registerPending(); err != nil {
				a.browser.SetStatus("Register failed: " + err.Error())
			} else {
				a.browser.SetStatus("Registered project: " + a.state.PendingRegisterName)
			}
		}
		// any other key dismisses without registering
		a.state.Screen = ScreenBrowser
		return a, nil
	case ScreenDashboard:
		var cmd tea.Cmd
		a.dashboard, cmd = a.dashboard.Update(msg)
		return a, cmd
	case ScreenProjectForm:
		var cmd tea.Cmd
		a.projectForm, cmd = a.projectForm.Update(msg)
		return a, cmd
	case ScreenBrowser:
		var cmd tea.Cmd
		a.browser, cmd = a.browser.Update(msg)
		return a, cmd
	case ScreenHostSelector:
		var cmd tea.Cmd
		a.hostSel, cmd = a.hostSel.Update(msg)
		return a, cmd
	case ScreenHostManager:
		var cmd tea.Cmd
		a.hostManager, cmd = a.hostManager.Update(msg)
		return a, cmd
	case ScreenHostForm:
		var cmd tea.Cmd
		a.hostForm, cmd = a.hostForm.Update(msg)
		return a, cmd
	case ScreenDiffView:
		var cmd tea.Cmd
		a.diffView, cmd = a.diffView.Update(msg)
		return a, cmd
	case ScreenDiffLoading:
		if key, ok := msg.(tea.KeyMsg); ok {
			if key.String() == "esc" || key.String() == "q" {
				a.state.Screen = ScreenBrowser
				if a.state.SelectedHost != nil {
					h := *a.state.SelectedHost
					return a, a.browser.StartRemote(h)
				}
			}
		}
		return a, nil
	}

	return a, nil
}

func renderLoadingProgress(progress diffview.LoadProgress, pulse, width int) string {
	phase := progress.Phase
	if phase == "" {
		phase = "Preparing…"
	}

	barWidth := width - 20
	if barWidth > 42 {
		barWidth = 42
	}
	if barWidth < 10 {
		barWidth = 10
	}

	var bar string
	var suffix string
	if progress.Total > 0 && !progress.Indeterminate {
		if progress.Done > progress.Total {
			progress.Done = progress.Total
		}
		filled := progress.Done * barWidth / progress.Total
		bar = styles.Marked.Render(strings.Repeat("█", filled)) + styles.Sep.Render(strings.Repeat("░", barWidth-filled))
		percent := progress.Done * 100 / progress.Total
		suffix = fmt.Sprintf(" %3d%%  %d/%d", percent, progress.Done, progress.Total)
	} else {
		segment := 5
		if segment > barWidth {
			segment = barWidth
		}
		span := barWidth - segment + 1
		pos := 0
		if span > 0 {
			pos = pulse % span
		}
		bar = styles.Sep.Render(strings.Repeat("░", pos)) +
			styles.Marked.Render(strings.Repeat("█", segment)) +
			styles.Sep.Render(strings.Repeat("░", barWidth-pos-segment))
		suffix = " …"
	}

	line := "  " + styles.Muted.Render(phase) + "\n" + "  [" + bar + "]" + styles.Muted.Render(suffix)
	if width > 0 && lipgloss.Width(line) > width {
		return lipgloss.NewStyle().MaxWidth(width).Render(line)
	}
	return line
}

func (a App) View() string {
	switch a.state.Screen {
	case ScreenRegisterPrompt:
		box := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(styles.ColorDir).
			Padding(1, 2)
		content := styles.Header.Render("Register this project?") + "\n\n" +
			styles.File.Render(a.state.PendingRegisterName) + "  " +
			styles.Muted.Render(a.state.PendingRegisterPath) + "\n\n" +
			styles.Key.Render("[y]") + styles.Muted.Render(" register   ") +
			styles.Key.Render("[n]") + styles.Muted.Render(" skip")
		return lipgloss.Place(
			a.state.TermWidth, a.state.TermHeight,
			lipgloss.Center, lipgloss.Center,
			box.Render(content),
		)
	case ScreenDashboard:
		return a.dashboard.View()
	case ScreenProjectForm:
		return a.projectForm.View()
	case ScreenBrowser:
		return a.browser.View()
	case ScreenHostSelector:
		return lipgloss.Place(
			a.state.TermWidth,
			a.state.TermHeight,
			lipgloss.Center,
			lipgloss.Center,
			a.hostSel.View(),
		)
	case ScreenDiffLoading:
		host := ""
		if a.state.SelectedHost != nil {
			host = a.state.SelectedHost.Hostname
		}
		return styles.Header.Render("drift") + "\n\n" +
			styles.Muted.Render("  Loading diffs for "+host+"…") + "\n\n" +
			renderLoadingProgress(a.diffLoadProgress, a.diffLoadPulse, a.state.TermWidth) + "\n\n" +
			styles.Muted.Render("  [Esc] cancel")
	case ScreenDiffView:
		return a.diffView.View()
	case ScreenHostManager:
		return a.hostManager.View()
	case ScreenHostForm:
		return a.hostForm.View()
	default:
		return ""
	}
}
