package tui

import (
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/dashboard"
	"github.com/WariKoda/drift/internal/tui/diffview"
	"github.com/WariKoda/drift/internal/tui/hostform"
	"github.com/WariKoda/drift/internal/tui/hostmanager"
	"github.com/WariKoda/drift/internal/tui/hostselector"
	"github.com/WariKoda/drift/internal/tui/loading"
	"github.com/WariKoda/drift/internal/tui/projectform"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

type networkActivity int

const (
	activityNone networkActivity = iota
	activityRemoteLoad
	activityDiffLoad
	activityHostTest
	activityDiffView
)

// App is the root bubbletea Model.
type App struct {
	state       AppState
	browser     browser.Model
	hostManager hostmanager.Model
	hostForm    hostform.Model
	hostSel     hostselector.Model
	diffView    diffview.Model
	diffLoading bool
	loader      loading.Model
	activity    networkActivity
	globalError string
	dashboard   dashboard.Model
	projectForm projectform.Model

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

func (a *App) startNetworkActivity(kind networkActivity, label string, tracker *loading.Tracker) tea.Cmd {
	a.activity = kind
	a.globalError = ""
	return a.loader.Start(label, tracker)
}

func (a *App) finishNetworkActivity(kind networkActivity) {
	if a.activity != kind {
		return
	}
	a.loader.Finish()
	a.activity = activityNone
}

func (a App) blocksNetworkKey(key tea.KeyMsg) bool {
	switch a.state.Screen {
	case ScreenBrowser:
		return a.browser.StartsNetworkOperation(key)
	case ScreenHostManager:
		return a.hostManager.StartsNetworkOperation(key)
	case ScreenHostSelector:
		return key.String() == "enter"
	default:
		return false
	}
}

func (a App) blocksQuitKey(key tea.KeyMsg) bool {
	if key.String() != "q" {
		return false
	}
	return a.state.Screen == ScreenBrowser || a.state.Screen == ScreenDashboard
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
	if cmd := a.loader.Update(msg); cmd != nil {
		return a, cmd
	}

	if key, ok := msg.(tea.KeyMsg); ok {
		if a.loader.Active() && key.String() == "ctrl+c" {
			return a, tea.Quit
		}
		if a.loader.Visible() {
			if key.String() == "esc" {
				a.loader.Hide()
			}
			return a, nil
		}
		if a.loader.Active() && (a.blocksQuitKey(key) || a.blocksNetworkKey(key)) {
			return a, nil
		}
		if a.globalError != "" {
			a.globalError = ""
		}
	}

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
			a.diffLoading = true
			a.state.SelectedHost = &h
			return a, tea.Batch(
				diffview.LoadCmd(h, a.state.Selection, a.state.RemoteSelection, a.state.Config, msg.Conn, tracker),
				a.startNetworkActivity(activityDiffLoad, "Loading diffs…", tracker),
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
		a.state.Screen = ScreenBrowser
		if a.state.HostSelectorPurpose == HostSelectorForRemoteBrowse {
			cmd := a.browser.StartRemote(h)
			return a, tea.Batch(cmd, a.startNetworkActivity(activityRemoteLoad, "Connecting to "+h.Name+"…", nil))
		}
		tracker := diffview.NewLoadProgressTracker()
		a.diffLoading = true
		a.state.SelectedHost = &h
		return a, tea.Batch(
			diffview.LoadCmd(h, a.state.Selection, a.state.RemoteSelection, a.state.Config, nil, tracker),
			a.startNetworkActivity(activityDiffLoad, "Loading diffs…", tracker),
		)

	case hostselector.MsgSelectorCancelled:
		a.state.Screen = ScreenBrowser
		return a, nil

	case browser.MsgRemoteLoaded:
		a.finishNetworkActivity(activityRemoteLoad)
		if msg.Err != nil {
			a.globalError = msg.Err.Error()
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

	// ── Diff loaded ───────────────────────────────────────────────────
	case diffview.MsgDiffLoaded:
		if !a.diffLoading || a.state.SelectedHost == nil {
			if msg.Conn != nil {
				_ = msg.Conn.Close()
			}
			return a, nil
		}
		a.diffLoading = false
		a.finishNetworkActivity(activityDiffLoad)
		a.diffView = diffview.New(
			msg.Sessions,
			*a.state.SelectedHost,
			msg.Conn, // connection stays open for sync ops
			a.state.TermWidth,
			a.state.TermHeight,
		)
		failed := 0
		for _, session := range msg.Sessions {
			if session.Err != nil {
				failed++
			}
		}
		if failed > 0 {
			a.globalError = fmt.Sprintf("Comparison failed for %d file(s)", failed)
		}
		a.state.Screen = ScreenDiffView
		return a, nil

	case diffview.MsgDiffError:
		log.Error("diff load failed", "err", msg.Err)
		if !a.diffLoading {
			return a, nil
		}
		a.diffLoading = false
		a.finishNetworkActivity(activityDiffLoad)
		a.globalError = "Diff comparison failed: " + msg.Err.Error()
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
			cmd := a.browser.StartRemote(h)
			return a, tea.Batch(cmd, a.startNetworkActivity(activityRemoteLoad, "Connecting to "+h.Name+"…", nil))
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

	case hostmanager.MsgTestResult:
		a.finishNetworkActivity(activityHostTest)
		if msg.Err != nil {
			a.globalError = "Connection test failed: " + msg.Err.Error()
		}
		var cmd tea.Cmd
		a.hostManager, cmd = a.hostManager.Update(msg)
		return a, cmd

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
		_, wasLoading := a.browser.LoadingActivity()
		var cmd tea.Cmd
		a.browser, cmd = a.browser.Update(msg)
		label, isLoading := a.browser.LoadingActivity()
		if !wasLoading && isLoading && !a.loader.Active() {
			return a, tea.Batch(cmd, a.startNetworkActivity(activityRemoteLoad, label, nil))
		}
		return a, cmd
	case ScreenHostSelector:
		var cmd tea.Cmd
		a.hostSel, cmd = a.hostSel.Update(msg)
		return a, cmd
	case ScreenHostManager:
		_, wasTesting := a.hostManager.Testing()
		var cmd tea.Cmd
		a.hostManager, cmd = a.hostManager.Update(msg)
		target, isTesting := a.hostManager.Testing()
		if !wasTesting && isTesting && !a.loader.Active() {
			return a, tea.Batch(cmd, a.startNetworkActivity(activityHostTest, "Testing "+target+"…", nil))
		}
		return a, cmd
	case ScreenHostForm:
		var cmd tea.Cmd
		a.hostForm, cmd = a.hostForm.Update(msg)
		return a, cmd
	case ScreenDiffView:
		_, _, wasLoading := a.diffView.LoadingActivity()
		var cmd tea.Cmd
		a.diffView, cmd = a.diffView.Update(msg)
		label, tracker, isLoading := a.diffView.LoadingActivity()
		if !wasLoading && isLoading && !a.loader.Active() {
			return a, tea.Batch(cmd, a.startNetworkActivity(activityDiffView, label, tracker))
		}
		if wasLoading && !isLoading {
			a.finishNetworkActivity(activityDiffView)
		}
		switch result := msg.(type) {
		case diffview.MsgSyncError:
			a.globalError = result.Err.Error()
		case diffview.MsgSessionReloaded:
			if result.Err != nil {
				a.globalError = "Diff refresh failed: " + result.Err.Error()
			}
		case diffview.MsgRefreshed:
			failed := 0
			for _, session := range result.Sessions {
				if session.Err != nil {
					failed++
				}
			}
			if failed > 0 {
				a.globalError = fmt.Sprintf("Diff refresh failed for %d file(s)", failed)
			}
		}
		return a, cmd
	}

	return a, nil
}

func (a App) baseView() string {
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

// View renders the active screen and then applies global activity/error UI.
func (a App) View() string {
	base := a.baseView()
	if a.loader.Visible() {
		return a.loader.Overlay(base, a.state.TermWidth, a.state.TermHeight)
	}
	if a.loader.BackgroundVisible() {
		return replaceStatusLine(base, a.state.TermWidth, a.state.TermHeight,
			"⏳ "+a.loader.Status()+" — running in background", false)
	}
	if a.globalError != "" {
		return replaceStatusLine(base, a.state.TermWidth, a.state.TermHeight,
			"Error: "+a.globalError, true)
	}
	return base
}

func replaceStatusLine(view string, width, height int, text string, isError bool) string {
	if height <= 0 {
		return view
	}
	lines := strings.Split(view, "\n")
	for len(lines) < height {
		lines = append(lines, "")
	}
	if len(lines) > height {
		lines = lines[:height]
	}
	if width > 4 && lipgloss.Width(text) > width-4 {
		text = ansi.Truncate(text, max(1, width-5), "") + "…"
	}
	line := "  " + text
	if isError {
		line = styles.Err.Render(line)
	} else {
		line = styles.Warn.Render(line)
	}
	line += strings.Repeat(" ", max(0, width-lipgloss.Width(line)))
	lines[height-1] = line
	return strings.Join(lines, "\n")
}
