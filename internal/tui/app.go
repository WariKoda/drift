package tui

import (
	"github.com/charmbracelet/lipgloss"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/yourusername/drift/internal/config"
	"github.com/yourusername/drift/internal/styles"
	"github.com/yourusername/drift/internal/tui/browser"
	"github.com/yourusername/drift/internal/tui/diffview"
	"github.com/yourusername/drift/internal/tui/hostform"
	"github.com/yourusername/drift/internal/tui/hostmanager"
	"github.com/yourusername/drift/internal/tui/hostselector"
)

// App is the root bubbletea Model.
type App struct {
	state       AppState
	browser     browser.Model
	hostManager hostmanager.Model
	hostForm    hostform.Model
	hostSel     hostselector.Model
	diffView    diffview.Model
}

// New creates a fully initialised App.
func New(workDir string, cfg *config.MergedConfig) (App, error) {
	b, err := browser.New(workDir)
	if err != nil {
		return App{}, err
	}
	return App{
		state: AppState{
			Screen:     ScreenBrowser,
			WorkingDir: workDir,
			Config:     cfg,
			Selection:  b.Selection,
		},
		browser: b,
	}, nil
}

func (a App) Init() tea.Cmd { return a.browser.Init() }

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
		return a, nil

	// ── Browser → Host Selector ───────────────────────────────────────
	case browser.MsgSyncRequested:
		a.state.Selection = msg.Selection
		a.hostSel = hostselector.New(a.state.Config, a.state.TermWidth, a.state.TermHeight)
		a.state.Screen = ScreenHostSelector
		return a, nil

	// ── Host chosen → start loading diffs ────────────────────────────
	case hostselector.MsgHostChosen:
		h := msg.Host
		a.state.SelectedHost = &h
		a.state.Screen = ScreenDiffLoading
		return a, diffview.LoadCmd(h, a.state.Selection, a.state.Config)

	case hostselector.MsgSelectorCancelled:
		a.state.Screen = ScreenBrowser
		return a, nil

	// ── Diff loaded ───────────────────────────────────────────────────
	case diffview.MsgDiffLoaded:
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
		a.state.Screen = ScreenBrowser
		a.browser.SetStatus("Connection failed: " + msg.Err.Error())
		return a, nil

	// ── Diff view → back to browser ───────────────────────────────────
	case diffview.MsgBackToBrowser:
		a.diffView.Close()
		a.state.Screen = ScreenBrowser
		a.state.Selection.Clear()
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
			}
		}
		return a, nil
	}

	return a, nil
}

func (a App) View() string {
	switch a.state.Screen {
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
			styles.Muted.Render("  Connecting to "+host+" and loading diffs…\n\n") +
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
