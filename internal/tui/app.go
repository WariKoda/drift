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
	mousepkg "github.com/WariKoda/drift/internal/tui/mouse"
	"github.com/WariKoda/drift/internal/tui/projectform"
	"github.com/WariKoda/drift/internal/tui/projectselector"
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
	loader      loading.Model
	activity    networkActivity
	globalError string

	// secretWarning reports that the project config just written is reachable
	// by Git. It stays on screen until the user leaves the host screens, since
	// a single frame is too easy to miss.
	secretWarning string

	dashboard   dashboard.Model
	projectForm projectform.Model
	projectSel  projectselector.Model

	// diffRequest is the ID of the diff request whose result the app still
	// wants; 0 means none is pending. diffSeq issues those IDs. Results of
	// abandoned requests are discarded instead of being applied to whatever
	// state the app has moved on to.
	diffRequest uint64
	diffSeq     uint64

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
	a.bindActiveProject(workDir, cfg)
	if a.state.ActiveProject != nil {
		a.recordOpened(a.state.ActiveProject.Slug)
	}

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
		OpenedAt:  now,
	}
	if err := a.registry.Add(p); err != nil {
		return err
	}
	if err := a.store.Save(a.registry); err != nil {
		return err
	}
	pc := p
	a.state.ActiveProject = &pc
	a.browser.SetProjectName(p.Name)
	return nil
}

func (a App) Init() tea.Cmd {
	if a.state.Screen == ScreenDashboard {
		return a.dashboard.Init()
	}
	return tea.Batch(a.browser.Init(), a.migrateProjectSecrets())
}

// migrateProjectSecrets moves credentials that a hand-written or pre-0.1.6
// .drift/config.toml still carries into the access store, and folds a
// 0.1.6-alpha secrets.toml into it. It also asks git whether that config was
// reachable, because a migrated password may already be in the repository's
// history.
//
// Both conditions matter. Gating on the project config alone left a
// 0.1.6-alpha secrets.toml untouched — its configs were already clean — so
// nothing read those credentials any more and every host that relied on one
// stopped connecting.
func (a App) migrateProjectSecrets() tea.Cmd {
	if a.state.Config == nil {
		return nil
	}
	if len(a.state.Config.ProjectSecretsInFile) == 0 && !a.state.Config.LegacySecretStore {
		return nil
	}
	root := a.state.Config.ProjectRoot
	return func() tea.Msg {
		n, err := config.MigrateProjectSecrets(root)
		return msgSecretsMigrated{Count: n, Exposure: config.ProjectConfigExposure(root), Err: err}
	}
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

// beginDiffRequest issues the ID for a new diff request and makes it the only
// one whose result the app will accept.
func (a *App) beginDiffRequest() uint64 {
	a.diffSeq++
	a.diffRequest = a.diffSeq
	return a.diffRequest
}

// abandonDiffRequest gives up on the pending diff request. Its result is
// discarded and its connection closed once it arrives.
func (a *App) abandonDiffRequest() {
	if a.diffRequest == 0 {
		return
	}
	a.diffRequest = 0
	a.finishNetworkActivity(activityDiffLoad)
}

// acceptsDiffResult reports whether a result with requestID belongs to the
// request the app is still waiting for.
func (a App) acceptsDiffResult(requestID uint64) bool {
	return requestID != 0 && requestID == a.diffRequest
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
// fresh browser at p.Path and switches to the browser screen. A diff request
// still in flight belongs to the project being left and is abandoned.
func (a *App) openProject(p project.Project) (tea.Cmd, error) {
	a.abandonDiffRequest()
	a.state.SelectedHost = nil
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
	b.SetProjectName(p.Name)

	a.browser = b
	a.state.Config = cfg
	a.state.WorkingDir = p.Path
	a.state.Selection = b.Selection
	a.state.RemoteSelection = b.RemoteSelection
	pc := p
	a.state.ActiveProject = &pc
	a.state.Screen = ScreenBrowser
	a.secretWarning = ""
	a.recordOpened(p.Slug)
	return tea.Batch(b.Init(), a.migrateProjectSecrets()), nil
}

// projectRoot is the loaded project root, or "" when no config is loaded.
func (a App) projectRoot() string {
	if a.state.Config == nil {
		return ""
	}
	return a.state.Config.ProjectRoot
}

func (a App) currentSlug() string {
	if a.state.ActiveProject == nil {
		return ""
	}
	return a.state.ActiveProject.Slug
}

// bindActiveProject sets ActiveProject and the browser header from the registry.
func (a *App) bindActiveProject(workDir string, cfg *config.MergedConfig) {
	if a.registry == nil {
		return
	}
	var p *project.Project
	if cfg != nil && cfg.ProjectRoot != "" {
		p = a.registry.FindByPath(cfg.ProjectRoot)
	}
	if p == nil {
		p = a.registry.FindByPath(workDir)
	}
	if p == nil {
		return
	}
	pc := *p
	a.state.ActiveProject = &pc
	a.browser.SetProjectName(p.Name)
}

// recordOpened stamps OpenedAt and persists it. A save failure is logged and
// does not undo the open — the session is already rooted in the project.
func (a *App) recordOpened(slug string) {
	if a.registry == nil || a.store == nil || slug == "" {
		return
	}
	existing := a.registry.Find(slug)
	if existing == nil {
		return
	}
	existing.OpenedAt = time.Now().UTC()
	pc := *existing
	a.state.ActiveProject = &pc
	if err := a.store.Save(a.registry); err != nil {
		log.Error("could not persist last-opened", "err", err, "slug", slug)
		return
	}
	a.dashboard.Refresh(a.registry)
}

// openPicker shows the project switcher over the current browser session.
// Remote pane and in-flight diffs stay; they are torn down only by openProject.
func (a *App) openPicker() {
	if a.store == nil || a.registry == nil {
		return
	}
	if reg, err := a.store.Load(); err == nil {
		a.registry = reg
	}
	a.projectSel = projectselector.New(
		a.registry.Active(), a.currentSlug(),
		a.state.TermWidth, a.state.TermHeight,
	)
	a.state.Screen = ScreenProjectSelector
}

// openDashboard shows the project CRUD screen. When returnable is true, Esc
// comes back to the browser instead of quitting.
func (a *App) openDashboard(returnable bool) {
	if a.registry == nil {
		return
	}
	a.dashboard.Refresh(a.registry)
	a.dashboard.SetSize(a.state.TermWidth, a.state.TermHeight)
	a.dashboard.SetReturnable(returnable)
	a.state.Screen = ScreenDashboard
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
		// The warning outlives ordinary keys on purpose — only esc dismisses it.
		if key.String() == "esc" {
			a.secretWarning = ""
		}
	}

	if mouse, ok := msg.(tea.MouseMsg); ok {
		// The modal covers the screen underneath, so nothing there is clickable.
		if a.loader.Visible() {
			return a, nil
		}
		// While network work runs, wheel scrolling stays allowed — it only moves
		// the viewport. Buttons could start a second operation, so they don't.
		if a.loader.Active() && !mousepkg.IsWheel(mouse) {
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
		a.projectSel.SetSize(msg.Width, msg.Height)
		return a, nil

	// ── Project Dashboard ─────────────────────────────────────────────
	case dashboard.MsgProjectChosen:
		cmd, err := a.openProject(msg.Project)
		if err != nil {
			a.dashboard.SetStatus("Cannot open project: " + err.Error())
			return a, nil
		}
		a.dashboard.SetReturnable(false)
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

	case dashboard.MsgDashboardBack:
		a.state.Screen = ScreenBrowser
		return a, nil

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

	// ── Browser → Project picker ──────────────────────────────────────
	case browser.MsgOpenDashboard:
		a.openPicker()
		return a, nil

	case projectselector.MsgSelectorCancelled:
		a.state.Screen = ScreenBrowser
		return a, nil

	case projectselector.MsgOpenDashboard:
		if a.store != nil {
			if reg, err := a.store.Load(); err == nil {
				a.registry = reg
			}
		}
		a.openDashboard(true)
		return a, nil

	case projectselector.MsgProjectChosen:
		if a.state.ActiveProject != nil && a.state.ActiveProject.Slug == msg.Project.Slug {
			a.state.Screen = ScreenBrowser
			return a, nil
		}
		cmd, err := a.openProject(msg.Project)
		if err != nil {
			a.projectSel.SetStatus("Cannot open project: " + err.Error())
			return a, nil
		}
		return a, cmd

	// ── Browser → Host Selector / direct sync ─────────────────────────
	case browser.MsgSyncRequested:
		a.state.Selection = msg.Selection
		a.state.RemoteSelection = msg.RemoteSelection
		if msg.Host != nil {
			h := *msg.Host
			tracker := diffview.NewLoadProgressTracker()
			return a, tea.Batch(
				diffview.LoadCmd(a.beginDiffRequest(), h,
					a.state.Selection, a.state.RemoteSelection, a.state.Config, msg.Conn, tracker),
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
		return a, tea.Batch(
			diffview.LoadCmd(a.beginDiffRequest(), h,
				a.state.Selection, a.state.RemoteSelection, a.state.Config, nil, tracker),
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
		if !a.acceptsDiffResult(msg.RequestID) {
			log.Info("discarding abandoned diff result", "host", msg.Host.Name, "request", msg.RequestID)
			if msg.Conn != nil {
				_ = msg.Conn.Close()
			}
			_ = msg.Root.Close()
			return a, nil
		}
		a.diffRequest = 0
		a.finishNetworkActivity(activityDiffLoad)
		// The host comes from the result, never from app state: sessions,
		// connection and host must belong to the same request.
		host := msg.Host
		a.state.SelectedHost = &host
		a.diffView = diffview.New(
			msg.Sessions,
			host,
			msg.Conn, // connection stays open for sync ops
			msg.Root, // project root stays open for local sync ops
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
		log.Error("diff load failed", "host", msg.Host.Name, "err", msg.Err)
		if !a.acceptsDiffResult(msg.RequestID) {
			return a, nil
		}
		a.diffRequest = 0
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
		a.secretWarning = ""
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

	case msgSecretsMigrated:
		if a.state.Config != nil {
			a.state.Config.ProjectSecretsInFile = nil
		}
		if msg.Err != nil {
			a.globalError = "Moving credentials out of the project failed: " + msg.Err.Error()
			log.Error("secret migration failed", "root", a.projectRoot(), "err", msg.Err)
			return a, nil
		}
		if msg.Count == 0 {
			return a, nil
		}
		a.secretWarning = migrationNotice(msg.Count, msg.Exposure)
		log.Info("moved project credentials to the secret store",
			"root", a.projectRoot(), "hosts", msg.Count, "notice", a.secretWarning)
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
	case ScreenProjectSelector:
		var cmd tea.Cmd
		a.projectSel, cmd = a.projectSel.Update(msg)
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
	case ScreenProjectSelector:
		return loading.OverlayCentered(
			a.browser.View(),
			a.projectSel.View(),
			a.state.TermWidth,
			a.state.TermHeight,
		)
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
	if a.secretWarning != "" {
		return replaceStatusLine(base, a.state.TermWidth, a.state.TermHeight,
			"⚠ "+a.secretWarning, false)
	}
	return base
}

// msgSecretsMigrated reports what the startup migration did, together with
// how reachable by git the config it read was.
type msgSecretsMigrated struct {
	Count    int
	Exposure config.Exposure
	Err      error
}

// migrationNotice tells the user where their credentials went, and — when the
// config they came from was reachable by git — that the old value may be in
// the repository's history and belongs rotated.
func migrationNotice(count int, e config.Exposure) string {
	subject := "credential"
	if count != 1 {
		subject = "credentials"
	}
	moved := fmt.Sprintf("Moved %d %s out of .drift/config.toml into %s",
		count, subject, config.AccessPathForDisplay())
	switch e {
	case config.ExposureTracked:
		return moved + " — git tracks that file, so treat the old value as leaked and rotate it: git rm --cached .drift/config.toml"
	case config.ExposureUntracked:
		return moved + " — that file was not ignored by git; check whether it ever got committed"
	default:
		return moved
	}
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
