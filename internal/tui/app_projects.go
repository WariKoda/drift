package tui

import (
	"fmt"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/project"
	"github.com/WariKoda/drift/internal/tui/browser"
	tea "github.com/charmbracelet/bubbletea"
)

type projectOpenRequest struct {
	id      uint64
	project project.Project
}

type msgProjectPrepared struct {
	request *projectOpenRequest
	config  *config.MergedConfig
	browser browser.Model
	err     error
}

type msgProjectOpened struct {
	previous *project.Registry
	registry *project.Registry
	err      error
}

// openProject prepares the new project off the UI thread. The existing session
// stays attached until preparation succeeds and the request is still wanted.
func (a *App) openProject(p project.Project) tea.Cmd {
	a.projectSeq++
	request := &projectOpenRequest{id: a.projectSeq, project: p}
	a.projectRequest = request
	return func() tea.Msg {
		result := msgProjectPrepared{request: request}
		result.config, result.err = config.Load(p.Path, p.Slug)
		if result.err == nil {
			result.browser, result.err = browser.New(p.Path)
		}
		if result.err == nil {
			result.browser.SetConfig(result.config)
		}
		if result.err != nil {
			log.Error("prepare project failed", "slug", p.Slug, "path", p.Path, "err", result.err)
		}
		return result
	}
}

func (a App) applyPreparedProject(msg msgProjectPrepared) (tea.Model, tea.Cmd) {
	if a.projectRequest == nil || msg.request == nil || msg.request.id != a.projectRequest.id {
		return a, nil
	}
	a.projectRequest = nil
	if msg.err != nil {
		status := "Cannot open project: " + msg.err.Error()
		if a.state.Screen == ScreenProjectSelector {
			a.projectSel.SetStatus(status)
		} else if a.state.Screen == ScreenDashboard {
			a.dashboard.SetStatus(status)
		} else {
			a.globalError = status
		}
		return a, nil
	}

	p := msg.request.project
	b := msg.browser
	b.SetSize(a.state.TermWidth, a.state.TermHeight)
	b.SetProjectName(p.Name)
	b.SetMouseEnabled(a.mouseEnabled)
	b.SetTrustManager(a.trust)

	a.abandonDiffRequest()
	a.cancelNetworkActivity()
	a.watchConnection(nil)
	a.state.SelectedHost = nil
	closeBrowser := a.browser.CloseRemote()
	closeDiff := a.diffView.Close()
	a.certPrompt = nil
	a.pendingTrust = nil
	a.trustSaving = false
	a.trustWriteSeq++

	a.browser.CancelLocal()
	a.browser = b
	a.state.Config = msg.config
	a.state.WorkingDir = p.Path
	a.state.Selection = b.Selection
	a.state.RemoteSelection = b.RemoteSelection
	a.state.ScopeOptions.IncludeIgnored = false
	a.pendingDiffStatus = ""
	a.pendingDiffErrors = nil
	a.state.ActiveProject = &p
	a.state.Screen = ScreenBrowser
	a.dashboard.SetReturnable(false)
	a.globalError = ""

	var recordOpened tea.Cmd
	if a.store != nil && a.registry != nil && p.Slug != "" {
		store, previous, openedAt := a.store, a.registry, time.Now().UTC()
		recordOpened = func() tea.Msg {
			result := msgProjectOpened{previous: previous}
			result.err = config.WithWriteLock(func(lock *config.WriteLock) error {
				reg, err := store.Load()
				if err != nil {
					return err
				}
				existing := reg.Find(p.Slug)
				if existing == nil {
					return fmt.Errorf("project %q not found", p.Slug)
				}
				if existing.Path != p.Path {
					return &config.ConflictError{Record: "project " + p.Slug}
				}
				if existing.OpenedAt.Before(openedAt) {
					existing.OpenedAt = openedAt
				}
				if err := store.SaveLocked(lock, reg); err != nil {
					return err
				}
				result.registry = reg
				return nil
			})
			if result.err != nil {
				log.Error("could not persist last-opened", "err", result.err, "slug", p.Slug)
				result.err = fmt.Errorf("record last-opened: %w", result.err)
			}
			return result
		}
	}
	return a, tea.Batch(closeBrowser, closeDiff, b.Init(), recordOpened)
}
