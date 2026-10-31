//! Window/session coordination. Child views own their interaction and load state.
use crate::certificates::CertificatePrompt;
use crate::design::{Palette, Text, metric, space};
use crate::{
    actions::*,
    browser::{BrowserCommand, BrowserEvent, BrowserPane},
    comparison::{ComparisonEvent, ComparisonPane},
    hosts::{HostEvent, HostManager},
    pane_split::{PaneSplit, SplitCommand},
    pane_tabs::{PaneSide, PaneTabs, PaneView},
    preview::{PreviewEvent, PreviewPane},
    projects::{ProjectEvent, ProjectsPanel},
    remote::{RemoteEvent, RemotePane},
    toolbar::{Toolbar, ToolbarEvent, ToolbarState},
};
use drift_app::{
    browser::{BrowserService, OperationId},
    hosts::{HostCommand, HostResponse},
    projects::{Launch, StartOptions},
    remote::RemoteService,
};
use drift_core::{store::Store, tlstrust::Manager};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AppContext, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
    PathPromptOptions, Render, Styled, Subscription, TestSupportExt, Window, div,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct Shell {
    preferences: Option<Entity<crate::preferences::Preferences>>,
    help: Option<keyboard::ShortcutHelp>,
    settings: Option<settings::SettingsScreen>,
    settings_serial: u64,
    certificate: Option<(u64, u64, Entity<CertificatePrompt>)>,
    certificate_subscription: Option<Subscription>,
    certificate_cancel: Option<CancellationToken>,
    comparison: Entity<ComparisonPane>,
    browser: Entity<BrowserPane>,
    local_preview: Entity<PreviewPane>,
    remote_preview: Entity<PreviewPane>,
    remote_preview_session: Option<OperationId>,
    projects: Entity<ProjectsPanel>,
    remote: Entity<RemotePane>,
    local_view: PaneView,
    remote_view: PaneView,
    local_focus: gpui_kit::FocusHandle,
    remote_focus: gpui_kit::FocusHandle,
    active_pane: PaneSide,
    split: PaneSplit,
    split_context: Option<(u64, PaneView, PaneView)>,
    navigation_epoch: u64,
    aligned_headers: aligned_header::AlignedHeaders,
    aligned_filters: aligned_header::AlignedHeaders,
    aligned_tabs: aligned_header::AlignedHeaders,
    hosts: Option<Entity<HostManager>>,
    store: Store,
    service: BrowserService,
    remote_service: RemoteService,
    configuration: u64,
    config_cancel: Option<(OperationId, CancellationToken)>,
    registration: Option<(OperationId, CancellationToken)>,
    folder_prompt: bool,
    startup: Option<CancellationToken>,
    start_directory: PathBuf,
    status: String,
    logging_warning: Option<String>,
    host_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}
impl Drop for Shell {
    fn drop(&mut self) {
        if let Some(cancel) = self.startup.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.certificate_cancel.take() {
            cancel.cancel();
        }
        if let Some((_, cancel)) = self.config_cancel.take() {
            cancel.cancel();
        }
        if let Some((_, cancel)) = self.registration.take() {
            cancel.cancel();
        }
    }
}
impl Shell {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        store: Store,
        service: BrowserService,
        remote_service: RemoteService,
        launch: impl Into<Launch>,
    ) -> Self {
        let launch = launch.into();
        let start = match &launch {
            Launch::Directory(path) => path.clone(),
            Launch::Automatic(options) => options.directory.clone(),
        };
        let trust = remote_service
            .trust_manager()
            .unwrap_or_else(|| Arc::new(Manager::new(store.clone())));
        let remote_service = remote_service.with_trust(trust);
        let browser = cx.new(|cx| {
            BrowserPane::new(store.clone(), service.clone(), start.clone(), window, cx)
                .with_external_chrome()
        });
        let local_preview =
            cx.new(|cx| PreviewPane::new(service.clone(), window, cx).with_external_chrome());
        let remote_preview =
            cx.new(|cx| PreviewPane::new(service.clone(), window, cx).with_external_chrome());
        let projects = cx.new(|cx| ProjectsPanel::new(store.clone(), service.clone(), window, cx));
        let remote =
            cx.new(|cx| RemotePane::new(remote_service.clone(), window, cx).with_external_chrome());
        let comparison = cx.new(|cx| ComparisonPane::new(service.clone(), window, cx));
        let mut subscriptions = vec![
            cx.observe(&browser, |_, _, cx| cx.notify()),
            cx.observe(&remote, |_, _, cx| cx.notify()),
            cx.subscribe_in(&comparison, window, |this, _, event, window, cx| {
                match event {
                    ComparisonEvent::FilesChanged {
                        project,
                        connection,
                    } if *project == this.browser.read(cx).id().project => {
                        this.clear_local_preview(window, cx);
                        this.clear_remote_preview(window, cx);
                        this.browser
                            .update(cx, |browser, cx| browser.refresh(&Refresh, window, cx));
                        if Some(*connection) == this.remote.read(cx).session_id() {
                            this.remote
                                .update(cx, |remote, cx| remote.refresh(&Refresh, window, cx));
                        }
                    }
                    ComparisonEvent::Status {
                        project,
                        connection,
                        message,
                    } if *project == this.browser.read(cx).id().project
                        && Some(*connection) == this.remote.read(cx).session_id() =>
                    {
                        this.status.clone_from(message)
                    }
                    ComparisonEvent::Closed => this.focus_workspace(window, cx),
                    _ => {}
                }
                cx.notify();
            }),
            cx.subscribe_in(&remote, window, |this, _, event, window, cx| {
                match event {
                    RemoteEvent::Certificate {
                        project,
                        connection,
                        challenge,
                    } => this.certificate_challenge(
                        *project,
                        *connection,
                        *challenge.clone(),
                        window,
                        cx,
                    ),
                    RemoteEvent::ListingChanged { id, connection } => {
                        let (project, current_connection, operation, ..) =
                            this.remote.read(cx).navigation_stamp();
                        if id.project != this.browser.read(cx).id().project
                            || id.project != project
                            || *connection != current_connection
                            || id.operation != operation
                        {
                            return;
                        }
                        this.clear_remote_preview(window, cx);
                    }
                    RemoteEvent::SelectionChanged {
                        project,
                        connection,
                    } => {
                        if *project != this.browser.read(cx).id().project
                            || *connection != this.remote.read(cx).connection()
                        {
                            return;
                        }
                        if this
                            .certificate
                            .as_ref()
                            .is_some_and(|(p, c, _)| (*p, *c) != (*project, *connection))
                        {
                            this.close_certificate(window, cx);
                        }
                        let session = this.remote.read(cx).session_id();
                        this.comparison
                            .update(cx, |pane, cx| pane.context(*project, session, cx));
                        if this.remote_preview_session.is_some()
                            && this.remote_preview_session != session
                        {
                            this.clear_remote_preview(window, cx);
                        }
                    }
                    RemoteEvent::Status {
                        project,
                        connection,
                        message,
                    } => {
                        if *project != this.browser.read(cx).id().project
                            || *connection != this.remote.read(cx).connection()
                        {
                            return;
                        }
                        this.status.clone_from(message);
                    }
                    RemoteEvent::Compare {
                        project,
                        connection,
                        path,
                    } => {
                        if *project != this.browser.read(cx).id().project
                            || *project != this.remote.read(cx).project()
                            || *connection != this.remote.read(cx).connection()
                            || this.remote.read(cx).selected() != Some(path.as_str())
                            || this.remote_view != PaneView::Files
                            || !this.browser_screen_active(cx)
                        {
                            return;
                        }
                        this.open_comparison(vec![], vec![path.clone()], window, cx);
                    }
                    RemoteEvent::Toolbar {
                        project,
                        connection,
                        event,
                    } => {
                        if *project != this.browser.read(cx).id().project
                            || *project != this.remote.read(cx).project()
                            || *connection != this.remote.read(cx).connection()
                            || this.remote_view != PaneView::Files
                            || !this.browser_screen_active(cx)
                        {
                            return;
                        }
                        this.toolbar_event(event, window, cx);
                    }
                    RemoteEvent::Preview {
                        session,
                        path,
                        project,
                    } => {
                        if *project != this.browser.read(cx).id().project
                            || Some(session.id) != this.remote.read(cx).session_id()
                            || this.remote.read(cx).selected() != Some(path.as_str())
                            || !this.browser_screen_active(cx)
                        {
                            return;
                        }
                        this.remote_preview_session = Some(session.id);
                        this.remote_preview.update(cx, |preview, cx| {
                            preview.show_remote(session.clone(), path.clone(), *project, window, cx)
                        });
                        this.status = "Loading remote preview…".into();
                        this.reveal_preview(PaneSide::Remote, window, cx);
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(&browser, window, |this, _, event, window, cx| {
                this.browser_event(event, window, cx)
            }),
            cx.subscribe_in(
                &projects,
                window,
                |this, _, event, window, cx| match event {
                    ProjectEvent::Open(path) => this.open_project(path.clone(), window, cx),
                    ProjectEvent::Register(name) => this.register(name.clone(), window, cx),
                    ProjectEvent::Changed(response) => this.project_changed(response, window, cx),
                    ProjectEvent::Close => this.close_projects(window, cx),
                },
            ),
        ];
        for (side, preview) in [
            (PaneSide::Local, &local_preview),
            (PaneSide::Remote, &remote_preview),
        ] {
            subscriptions.push(cx.subscribe_in(
                preview,
                window,
                move |this, _, event, window, cx| {
                    this.preview_event(side, event, window, cx);
                },
            ));
        }
        let mut logging_failures = service.logger().failures();
        let logging_warning = logging_failures.borrow_and_update().clone();
        cx.spawn(async move |this, cx| {
            while logging_failures.changed().await.is_ok() {
                let warning = logging_failures.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| {
                        this.logging_warning = warning;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut shell = Self {
            preferences: None,
            help: None,
            settings: None,
            settings_serial: 0,
            certificate: None,
            certificate_subscription: None,
            certificate_cancel: None,
            comparison,
            browser,
            local_preview,
            remote_preview,
            remote_preview_session: None,
            projects,
            remote,
            local_view: PaneView::Files,
            remote_view: PaneView::Files,
            local_focus: cx.focus_handle(),
            remote_focus: cx.focus_handle(),
            active_pane: PaneSide::Local,
            split: PaneSplit::new(None, cx),
            split_context: None,
            navigation_epoch: 0,
            aligned_headers: aligned_header::AlignedHeaders::new("header", 3.),
            aligned_filters: aligned_header::AlignedHeaders::new("filter", 1.),
            aligned_tabs: aligned_header::AlignedHeaders::bordered("tabs", 1.),
            hosts: None,
            store,
            service,
            remote_service,
            configuration: 0,
            config_cancel: None,
            registration: None,
            folder_prompt: false,
            startup: None,
            start_directory: start.clone(),
            status: "Opening project…".into(),
            logging_warning,
            host_subscription: None,
            _subscriptions: subscriptions,
        };
        match launch {
            Launch::Directory(path) => shell
                .browser
                .update(cx, |browser, cx| browser.open(path, window, cx)),
            Launch::Automatic(options) => shell.resolve_start(options, window, cx),
        }
        shell
    }
    fn browser_event(&mut self, event: &BrowserEvent, window: &mut Window, cx: &mut Context<Self>) {
        let id = match event {
            BrowserEvent::Changed(id)
            | BrowserEvent::Opened { id, .. }
            | BrowserEvent::Selected { id, .. }
            | BrowserEvent::Status { id, .. }
            | BrowserEvent::Compare { id, .. }
            | BrowserEvent::Toolbar { id, .. } => *id,
        };
        if id != self.browser.read(cx).id() {
            return;
        }
        match event {
            BrowserEvent::Changed(_) => {
                self.clear_local_preview(window, cx);
                if self
                    .certificate
                    .as_ref()
                    .is_some_and(|(p, _, _)| *p != id.project)
                {
                    self.close_certificate(window, cx);
                }
                let session = self.remote.read(cx).session_id();
                self.comparison
                    .update(cx, |pane, cx| pane.context(id.project, session, cx));
                if self.remote.read(cx).project() != id.project {
                    self.hosts = None;
                    self.host_subscription = None;
                    self.remote
                        .update(cx, |remote, cx| remote.set_context(id.project, vec![], cx));
                    self.local_view = PaneView::Files;
                    self.local_preview.update(cx, |preview, cx| {
                        preview.select(None, id.project, window, cx)
                    });
                    self.clear_remote_preview(window, cx);
                }
                if self
                    .registration
                    .as_ref()
                    .is_some_and(|(request, _)| *request != id)
                {
                    self.cancel_registration(cx);
                }
                if self
                    .config_cancel
                    .as_ref()
                    .is_some_and(|(request, _)| request.project != id.project)
                    && let Some((_, cancel)) = self.config_cancel.take()
                {
                    cancel.cancel();
                }
            }
            BrowserEvent::Opened { registry, .. } => {
                let hosts = self
                    .browser
                    .read(cx)
                    .location()
                    .map(|l| l.config.hosts.clone())
                    .unwrap_or_default();
                self.remote.update(cx, |remote, cx| {
                    remote.set_context(id.project, hosts, cx);
                    remote.set_selection_location(
                        self.browser.read(cx).location().cloned(),
                        window,
                        cx,
                    );
                });
                let can_register = self
                    .browser
                    .read(cx)
                    .location()
                    .is_some_and(|location| location.slug.is_none());
                self.projects.update(cx, |projects, cx| {
                    projects.set_context(registry.clone(), can_register, cx)
                });
            }
            BrowserEvent::Selected { path, preview, .. } => {
                if path.as_deref() != self.browser.read(cx).selected().map(std::path::Path::new) {
                    return;
                }
                let location = self.browser.read(cx).location().cloned();
                if *preview && let (Some(location), Some(path)) = (location, path) {
                    self.local_preview.update(cx, |preview, cx| {
                        preview.show(location, path.clone(), id.project, window, cx)
                    });
                    self.status = "Loading preview…".into();
                    self.reveal_preview(PaneSide::Local, window, cx);
                }
            }
            BrowserEvent::Compare {
                path, connection, ..
            } => {
                if Some(*connection) != self.remote.read(cx).session_id()
                    || self.local_view != PaneView::Files
                    || self.browser.read(cx).selected().map(std::path::Path::new)
                        != Some(path.as_path())
                    || !self.browser_screen_active(cx)
                {
                    return;
                }
                self.open_comparison(vec![path.clone()], vec![], window, cx);
            }
            BrowserEvent::Toolbar {
                event, connection, ..
            } => {
                if Some(*connection) != self.remote.read(cx).session_id()
                    || self.local_view != PaneView::Files
                    || !self.browser_screen_active(cx)
                {
                    return;
                }
                self.toolbar_event(event, window, cx);
            }
            BrowserEvent::Status { message, .. } => self.status.clone_from(message),
        }
        cx.notify();
    }
    fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_registration(cx);
        self.projects.update(cx, |projects, cx| projects.close(cx));
        if self
            .browser
            .read(cx)
            .location()
            .is_some_and(|location| location.slug.is_some() && location.root.base() == path)
        {
            self.focus_workspace(window, cx);
            cx.notify();
            return;
        }
        self.browser
            .update(cx, |browser, cx| browser.open(path, window, cx));
        cx.notify();
    }
    fn cancel_registration(&mut self, cx: &mut Context<Self>) {
        if let Some((_, cancel)) = self.registration.take() {
            cancel.cancel();
        }
        self.projects
            .update(cx, |projects, cx| projects.set_busy(false, cx));
    }
    fn register(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.browser.read(cx).location().cloned() else {
            return;
        };
        if name.trim().is_empty() {
            self.status = "Enter a project name".into();
            cx.notify();
            return;
        }
        self.cancel_registration(cx);
        self.browser
            .update(cx, |browser, cx| browser.stop_listing(cx));
        let id = self.browser.read(cx).id();
        let operation = self
            .service
            .register(self.store.clone(), name, location.directory, id);
        self.registration = Some((id, operation.cancel));
        self.projects
            .update(cx, |projects, cx| projects.set_busy(true, cx));
        self.status = "Registering project…".into();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.registration.as_ref().map(|(request, _)| *request) != Some(id)
                    || this.browser.read(cx).id() != id
                {
                    return;
                }
                this.cancel_registration(cx);
                match result {
                    Ok(Ok(path)) => this.open_project(path, window, cx),
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn open_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.deactivate_browser_navigation(cx);
        self.comparison
            .update(cx, |pane, cx| pane.deactivate_layout(window, cx));
        let slug = self
            .browser
            .read(cx)
            .location()
            .and_then(|location| location.slug.clone());
        let hosts = cx.new(|cx| {
            HostManager::new(
                self.store.clone(),
                self.service.clone(),
                self.remote_service.clone(),
                slug,
                window,
                cx,
            )
        });
        self.host_subscription =
            Some(
                cx.subscribe_in(&hosts, window, |this, _, event: &HostEvent, window, cx| {
                    match event {
                        HostEvent::Close => this.close_hosts(window, cx),
                        HostEvent::Changed => this.refresh_config(window, cx),
                    }
                    cx.notify();
                }),
            );
        self.hosts = Some(hosts);
        cx.notify();
    }
    fn close_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(hosts) = self.hosts.take() {
            hosts.update(cx, |hosts, _| hosts.deactivate());
        }
        self.host_subscription = None;
        self.focus_workspace(window, cx);
        cx.notify();
    }
    fn refresh_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.browser.read(cx).location().cloned() else {
            return;
        };
        self.configuration += 1;
        if let Some((_, cancel)) = self.config_cancel.take() {
            cancel.cancel();
        }
        let id = OperationId {
            project: self.browser.read(cx).id().project,
            operation: self.configuration,
        };
        let operation =
            self.service
                .manage_hosts(self.store.clone(), location.slug, HostCommand::Load, id);
        self.config_cancel = Some((id, operation.cancel));
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.browser.read(cx).id().project != id.project
                    || this.configuration != id.operation
                {
                    return;
                }
                this.config_cancel = None;
                match result {
                    Ok(Ok(HostResponse::Loaded(catalog))) => {
                        let hosts = catalog.runtime.hosts.clone();
                        this.browser.update(cx, |browser, cx| {
                            browser.replace_config(catalog.runtime, cx)
                        });
                        this.remote.update(cx, |remote, cx| {
                            remote.set_context(id.project, hosts, cx);
                            remote.set_selection_location(
                                this.browser.read(cx).location().cloned(),
                                window,
                                cx,
                            );
                        });
                    }
                    Ok(Ok(_)) => unreachable!("configuration load only returns a catalog"),
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Configuration refresh failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open folder".into()),
        });
        self.deactivate_browser_navigation(cx);
        self.folder_prompt = true;
        let project = self.browser.read(cx).id().project;
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.folder_prompt = false;
                if this.browser.read(cx).id().project == project {
                    match result {
                        Ok(Ok(Some(paths))) => {
                            if let Some(path) = paths.into_iter().next() {
                                this.open_project(path, window, cx);
                            }
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => this.status = error.to_string(),
                        Err(error) => this.status = format!("Folder picker failed: {error}"),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_screen_active(cx) {
            return;
        }
        let side = self.focused_side(window, cx);
        self.set_pane_view(side, PaneView::Files, window, cx);
        match side {
            PaneSide::Local => self
                .browser
                .update(cx, |pane, cx| pane.focus_filter_input(window, cx)),
            PaneSide::Remote => self
                .remote
                .update(cx, |pane, cx| pane.focus_filter(&FocusFilter, window, cx)),
        }
    }
    fn copy_selection(
        &mut self,
        action: &CopySelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) {
            return;
        }
        let side = self.focused_side(window, cx);
        if self.pane_view(side) == PaneView::Preview {
            self.preview_pane(side)
                .update(cx, |pane, cx| pane.copy_path(cx));
        } else {
            match side {
                PaneSide::Local => self
                    .browser
                    .update(cx, |pane, cx| pane.copy_selection(action, window, cx)),
                PaneSide::Remote => self
                    .remote
                    .update(cx, |pane, cx| pane.copy(action, window, cx)),
            }
        }
    }
    fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_screen_active(cx) {
            return;
        }
        self.refresh_side(self.focused_side(window, cx), window, cx);
    }
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.read(cx).visible() {
            self.projects
                .update(cx, |projects, cx| projects.cancel_view(&Cancel, window, cx));
        } else {
            let local = self.browser.read(cx).is_loading()
                || self.local_preview.read(cx).is_loading()
                || self.registration.is_some();
            let remote =
                self.remote.read(cx).is_loading() || self.remote_preview.read(cx).is_loading();
            if local {
                self.cancel_side(PaneSide::Local, window, cx);
            }
            if remote {
                self.cancel_side(PaneSide::Remote, window, cx);
            }
            if !local && !remote {
                self.set_pane_view(self.focused_side(window, cx), PaneView::Files, window, cx);
            }
        }
        cx.notify();
    }
    fn cancel_work(&mut self, _: &CancelWork, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_screen_active(cx) {
            return;
        }
        if self.browser.read(cx).is_loading()
            || self.remote.read(cx).is_loading()
            || self.local_preview.read(cx).is_loading()
            || self.remote_preview.read(cx).is_loading()
            || self.registration.is_some()
        {
            self.cancel(&Cancel, window, cx);
        }
    }
    fn toolbar_event(&mut self, event: &ToolbarEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.toolbar_state(cx).allows(event) {
            return;
        }
        match event {
            ToolbarEvent::Home => {
                self.settings = None;
                self.deactivate_browser_navigation(cx);
                if self.projects.read(cx).visible() {
                    self.projects.update(cx, |projects, cx| projects.close(cx));
                }
                if let Some(hosts) = self.hosts.take() {
                    hosts.update(cx, |hosts, _| hosts.deactivate());
                    self.host_subscription = None;
                }
                if self.comparison.read(cx).visible() {
                    self.comparison
                        .update(cx, |pane, cx| pane.close_idle(window, cx));
                }
                self.local_view = PaneView::Files;
                self.remote_view = PaneView::Files;
                self.active_pane = PaneSide::Local;
                if self.browser.read(cx).finder_active() {
                    self.browser.update(cx, |pane, cx| {
                        pane.command(BrowserCommand::ReturnFinder, window, cx)
                    });
                }
                if self.browser.read(cx).location().is_none() && !self.browser.read(cx).is_loading()
                {
                    self.open_project(self.start_directory.clone(), window, cx);
                } else {
                    self.browser.update(cx, |pane, cx| pane.focus(window, cx));
                }
            }
            ToolbarEvent::Projects => {
                let from_settings = self.settings.take().is_some();
                self.deactivate_browser_navigation(cx);
                if let Some(hosts) = self.hosts.take() {
                    hosts.update(cx, |hosts, _| hosts.deactivate());
                    self.host_subscription = None;
                }
                self.comparison
                    .update(cx, |pane, cx| pane.deactivate_layout(window, cx));
                self.projects.update(cx, |projects, cx| {
                    if from_settings && projects.visible() {
                        projects.focus(window, cx);
                    } else {
                        projects.toggle(window, cx);
                    }
                });
                if !self.projects.read(cx).visible() {
                    self.close_projects(window, cx);
                }
            }
            ToolbarEvent::Hosts => {
                // From Settings, Hosts returns to an open manager instead of closing it.
                let from_settings = self.settings.take().is_some();
                match &self.hosts {
                    Some(hosts) if from_settings => {
                        hosts.update(cx, |hosts, cx| hosts.focus(window, cx));
                    }
                    Some(_) => self.close_hosts(window, cx),
                    None => self.open_hosts(window, cx),
                }
            }
            ToolbarEvent::Settings => {
                if self.settings.is_some() {
                    self.close_settings(window, cx);
                } else {
                    self.open_settings(window, cx);
                }
            }
            ToolbarEvent::OpenFolder => self.open_folder(window, cx),
            ToolbarEvent::Cancel => self.cancel_side(PaneSide::Local, window, cx),
            ToolbarEvent::CancelRemote => self.cancel_side(PaneSide::Remote, window, cx),
            ToolbarEvent::CompareProject
            | ToolbarEvent::CompareLocal
            | ToolbarEvent::CompareRemote
            | ToolbarEvent::CompareMarked => self.compare(event, window, cx),
            ToolbarEvent::Browser(command) => {
                if matches!(command, BrowserCommand::Refresh) {
                    self.refresh_side(PaneSide::Local, window, cx);
                } else {
                    self.set_pane_view(PaneSide::Local, PaneView::Files, window, cx);
                    self.browser
                        .update(cx, |browser, cx| browser.command(*command, window, cx));
                }
            }
        }
        cx.notify();
    }
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(window, cx);
        let identity = (
            self.browser.read(cx).id().project,
            self.remote.read(cx).connection(),
        );
        let toolbar = Toolbar::new(
            self.toolbar_state(cx),
            cx.listener(move |this, event, window, cx| {
                if identity
                    == (
                        this.browser.read(cx).id().project,
                        this.remote.read(cx).connection(),
                    )
                {
                    this.toolbar_event(event, window, cx);
                }
            }),
        );
        let toolbar = div()
            .flex_shrink_0()
            .min_w_0()
            .key_context(if self.browser_screen_active(cx) {
                "Drift"
            } else {
                "DriftToolbar"
            })
            .capture_action(cx.listener(|this, _: &Cancel, w, cx| {
                if this.browser_screen_active(cx) {
                    this.split.cancel_resize(w, cx);
                } else {
                    cx.propagate();
                }
            }))
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, w, cx| {
                    if this.browser_screen_active(cx) {
                        this.split.cancel_resize(w, cx);
                    } else {
                        cx.propagate();
                    }
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::component::input::Escape, w, cx| {
                    if this.browser_screen_active(cx) {
                        this.split.cancel_resize(w, cx);
                    } else {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cancel_work))
            .on_action(cx.listener(Self::focus_preview))
            .on_action(cx.listener(Self::copy_preview))
            .on_action(cx.listener(|this, _: &Cancel, w, cx| {
                if this.browser_screen_active(cx) {
                    this.cancel(&Cancel, w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ReturnFromFinder, w, cx| {
                if this.browser_screen_active(cx) && this.browser.read(cx).finder_active() {
                    this.toolbar_event(&ToolbarEvent::Browser(BrowserCommand::ReturnFinder), w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ResizePaneLeft, w, cx| {
                if this.browser_screen_active(cx) {
                    this.split.command(SplitCommand::Left, w, cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ResizePaneRight, w, cx| {
                if this.browser_screen_active(cx) {
                    this.split.command(SplitCommand::Right, w, cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ResetPaneSizes, w, cx| {
                if this.browser_screen_active(cx) {
                    this.split.command(SplitCommand::Reset, w, cx);
                    cx.notify();
                }
            }))
            .child(toolbar);
        let mut view = div()
            .on_action(cx.listener(Self::show_help))
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(toolbar);
        if let Some(warning) = &self.logging_warning {
            view = view.child(
                div()
                    .id("logging-warning")
                    .test_support()
                    .flex_shrink_0()
                    .p_2()
                    .text_color(cx.theme().danger)
                    .child(warning.clone()),
            );
        }
        if let Some(warning) = self
            .preferences
            .as_ref()
            .and_then(|p| p.read(cx).warning.clone())
        {
            view = view.child(
                div()
                    .id("preferences-warning")
                    .test_support()
                    .flex_shrink_0()
                    .p_2()
                    .text_color(cx.theme().danger)
                    .child(warning),
            );
        }
        view.child(div().flex().flex_col().flex_1().min_h_0().child(content))
    }
}
impl Shell {
    fn toolbar_state(&self, cx: &gpui_kit::App) -> ToolbarState {
        let browser = self.browser.read(cx);
        let projects = self.projects.read(cx);
        let comparison = self.comparison.read(cx);
        let management_enabled = self.help.is_none()
            && self.certificate.is_none()
            && self.startup.is_none()
            && !self.folder_prompt
            && (!projects.visible() || projects.can_leave())
            && self
                .hosts
                .as_ref()
                .is_none_or(|hosts| hosts.read(cx).can_leave())
            && (!comparison.visible() || comparison.can_leave());
        ToolbarState {
            management_enabled,
            browser_enabled: self.browser_screen_active(cx),
            projects_open: projects.visible() && self.settings.is_none(),
            hosts_open: self.hosts.is_some() && self.settings.is_none(),
            settings_open: self.settings.is_some(),
            can_compare: self.remote.read(cx).has_session()
                && !self.remote.read(cx).is_loading()
                && !browser.is_loading(),
            has_location: browser.location().is_some(),
            listing: browser.is_loading(),
            cancellable: browser.is_loading()
                || self.local_preview.read(cx).is_loading()
                || self.registration.is_some(),
            remote_cancellable: self.remote.read(cx).is_loading()
                || self.remote_preview.read(cx).is_loading(),
            folder_prompt: self.folder_prompt,
            can_back: browser.can_back(),
            can_forward: browser.can_forward(),
            can_up: browser.can_up(),
            show_hidden: browser.shows_hidden(),
            show_ignored: browser.shows_ignored(),
        }
    }

    pub(super) fn deactivate_browser_navigation(&mut self, cx: &mut Context<Self>) {
        self.navigation_epoch += 1;
        self.browser
            .update(cx, |pane, cx| pane.set_navigation_active(false, cx));
        self.remote
            .update(cx, |pane, cx| pane.set_workspace_active(false, false, cx));
    }

    fn local_toolbar_active(&self, identity: (u64, u64), cx: &gpui_kit::App) -> bool {
        self.browser_screen_active(cx)
            && identity == (self.browser.read(cx).id().project, self.navigation_epoch)
    }

    fn remote_toolbar_active(&self, identity: (u64, u64, u64), cx: &gpui_kit::App) -> bool {
        self.local_toolbar_active((identity.0, identity.2), cx)
            && identity.1 == self.remote.read(cx).connection()
    }

    fn browser_screen_active(&self, cx: &gpui_kit::App) -> bool {
        self.help.is_none()
            && self.certificate.is_none()
            && self.settings.is_none()
            && self.hosts.is_none()
            && self.startup.is_none()
            && !self.folder_prompt
            && !self.projects.read(cx).visible()
            && !self.comparison.read(cx).visible()
    }

    fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let browser_active = self.browser_screen_active(cx);
        let split_context = browser_active.then_some((
            self.browser.read(cx).id().project,
            self.local_view,
            self.remote_view,
        ));
        if self.split_context != split_context {
            self.navigation_epoch += 1;
            self.split.deactivate(window, cx);
            self.split_context = split_context;
        }
        let comparison_active = self.help.is_none()
            && self.settings.is_none()
            && self.certificate.is_none()
            && self.hosts.is_none()
            && self.startup.is_none()
            && !self.projects.read(cx).visible()
            && self.comparison.read(cx).visible();
        if !comparison_active {
            self.comparison
                .update(cx, |pane, cx| pane.deactivate_layout(window, cx));
        }
        let can_compare = self.remote.read(cx).has_session()
            && !self.remote.read(cx).is_loading()
            && !self.browser.read(cx).is_loading();
        let comparison_ready =
            browser_active && can_compare && self.browser.read(cx).location().is_some();
        let local_visible = browser_active && self.local_view == PaneView::Files;
        let remote_visible = browser_active && self.remote_view == PaneView::Files;
        let comparison_connection = if comparison_ready && local_visible {
            self.remote.read(cx).session_id()
        } else {
            None
        };
        self.browser.update(cx, |pane, cx| {
            pane.set_navigation_active(local_visible, cx);
            pane.set_comparison_context(comparison_connection, window, cx);
            if !local_visible {
                pane.dismiss_context_menu(window, cx);
            }
        });
        self.remote.update(cx, |pane, cx| {
            pane.set_workspace_active(browser_active, remote_visible, cx);
            pane.set_comparison_ready(comparison_ready && remote_visible, window, cx);
            if !remote_visible {
                pane.dismiss_context_menu(window, cx);
            }
            // Capture the new workspace scope, not the observer's previous-frame picker.
            pane.sync_host_select(window, cx);
        });
        if self.help.is_some() {
            return self.render_help(window, cx);
        }
        if let Some((_, _, prompt)) = &self.certificate {
            return div().size_full().child(prompt.clone()).into_any_element();
        }
        if self.settings.is_some() {
            return self.render_settings(cx);
        }
        if let Some(hosts) = &self.hosts {
            return div().size_full().child(hosts.clone()).into_any_element();
        }
        if self.startup.is_some() {
            return div()
                .size_full()
                .p_3()
                .child(self.status.clone())
                .into_any_element();
        }
        if self.projects.read(cx).visible() {
            return div()
                .size_full()
                .child(self.projects.clone())
                .into_any_element();
        }
        if self.comparison.read(cx).visible() {
            let palette = Palette::current(cx);
            return div()
                .flex()
                .flex_col()
                .size_full()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(div().flex_1().min_h_0().child(self.comparison.clone()))
                .child(
                    div()
                        .id("workspace-status")
                        .text_size(Text::Metadata.rems())
                        .px(metric(space::PANEL, cx))
                        .py(metric(space::TIGHT, cx))
                        .bg(palette.chrome)
                        .border_t_1()
                        .border_color(palette.border)
                        .child(self.status.clone()),
                )
                .into_any_element();
        }
        let header = self.browser.update(cx, |browser, cx| browser.header(cx));
        let browser = self.browser.read(cx);
        let identity = (
            browser.id().project,
            self.remote.read(cx).connection(),
            self.navigation_epoch,
        );
        let local_identity = (identity.0, identity.2);
        let local_navigation_stamp = self.browser.read(cx).navigation_stamp();
        let remote_navigation_stamp = self.remote.read(cx).navigation_stamp();
        let local_tab_event = cx.listener(move |this, view: &PaneView, w, cx| {
            if this.local_toolbar_active(local_identity, cx) {
                this.set_pane_view(PaneSide::Local, *view, w, cx);
            }
        });
        let local_tabs = PaneTabs::new(
            PaneSide::Local,
            self.local_view,
            self.browser.read(cx).can_preview() || self.local_preview.read(cx).can_show(identity.0),
            "Local",
            move |view, w, cx| local_tab_event(&view, w, cx),
        );
        let remote_tab_event = cx.listener(move |this, view: &PaneView, w, cx| {
            if this.remote_toolbar_active(identity, cx) {
                this.set_pane_view(PaneSide::Remote, *view, w, cx);
            }
        });
        let remote_tabs = PaneTabs::new(
            PaneSide::Remote,
            self.remote_view,
            self.remote.read(cx).can_preview() || self.remote_preview.read(cx).can_show(identity.0),
            "Remote",
            move |view, w, cx| remote_tab_event(&view, w, cx),
        );
        // The pane title already says Local; only Finder needs its Return row.
        let local_actions = if self.local_view == PaneView::Preview {
            Some(
                self.local_preview
                    .update(cx, |pane, cx| pane.controls_header(cx)),
            )
        } else if self.browser.read(cx).finder_active() {
            Some(self.browser.update(cx, |pane, cx| pane.actions_header(cx)))
        } else {
            None
        };
        let local_path = if self.local_view == PaneView::Files {
            self.browser.update(cx, |pane, cx| pane.path_header(cx))
        } else {
            div()
                .id("local-path-header")
                .test_support()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .min_w_0()
                .h(metric(crate::design::size::HEADER, cx))
                .child(
                    self.local_preview
                        .update(cx, |pane, cx| pane.path_header(cx)),
                )
                .into_any_element()
        };
        let local_footer = self
            .browser
            .update(cx, |pane, cx| pane.selection_footer(cx));
        let remote_path = if self.remote_view == PaneView::Files {
            self.remote.update(cx, |pane, cx| pane.path_header(cx))
        } else {
            div()
                .id("remote-path-header")
                .test_support()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .min_w_0()
                .h(metric(crate::design::size::HEADER, cx))
                .child(
                    self.remote_preview
                        .update(cx, |pane, cx| pane.path_header(cx)),
                )
                .into_any_element()
        };
        let remote_preview_controls = (self.remote_view == PaneView::Preview).then(|| {
            self.remote_preview
                .update(cx, |pane, cx| pane.controls_header(cx))
        });
        let remote_footer = self.remote.update(cx, |pane, cx| pane.selection_footer(cx));
        let local_workspace = div()
            .id("local-workspace")
            .track_focus(&self.local_focus)
            .test_support()
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Back),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    if this.local_toolbar_active(local_identity, cx) {
                        this.browser.update(cx, |pane, cx| {
                            pane.mouse_navigation(
                                local_navigation_stamp,
                                gpui_kit::NavigationDirection::Back,
                                w,
                                cx,
                            )
                        });
                    }
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Forward),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    if this.local_toolbar_active(local_identity, cx) {
                        this.browser.update(cx, |pane, cx| {
                            pane.mouse_navigation(
                                local_navigation_stamp,
                                gpui_kit::NavigationDirection::Forward,
                                w,
                                cx,
                            )
                        });
                    }
                }),
            )
            .capture_any_mouse_down(cx.listener(
                move |this, _: &gpui_kit::MouseDownEvent, w, cx| {
                    if this.local_toolbar_active(local_identity, cx) {
                        this.active_pane = PaneSide::Local;
                    } else {
                        w.prevent_default();
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_any_mouse_up(cx.listener(move |this, _: &gpui_kit::MouseUpEvent, w, cx| {
                if !this.local_toolbar_active(local_identity, cx) {
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                self.aligned_headers.view(
                    PaneSide::Local,
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .min_w_0()
                        .on_mouse_down(
                            gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Back),
                            cx.listener(move |this, _, w, cx| {
                                cx.stop_propagation();
                                if this.local_toolbar_active(local_identity, cx) {
                                    this.browser.update(cx, |pane, cx| {
                                        pane.mouse_navigation(
                                            local_navigation_stamp,
                                            gpui_kit::NavigationDirection::Back,
                                            w,
                                            cx,
                                        )
                                    });
                                }
                            }),
                        )
                        .on_mouse_down(
                            gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Forward),
                            cx.listener(move |this, _, w, cx| {
                                cx.stop_propagation();
                                if this.local_toolbar_active(local_identity, cx) {
                                    this.browser.update(cx, |pane, cx| {
                                        pane.mouse_navigation(
                                            local_navigation_stamp,
                                            gpui_kit::NavigationDirection::Forward,
                                            w,
                                            cx,
                                        )
                                    });
                                }
                            }),
                        )
                        .capture_any_mouse_down(cx.listener(
                            move |this, _: &gpui_kit::MouseDownEvent, w, cx| {
                                if !this.local_toolbar_active(local_identity, cx) {
                                    w.prevent_default();
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .capture_any_mouse_up(cx.listener(
                            move |this, _: &gpui_kit::MouseUpEvent, w, cx| {
                                if !this.local_toolbar_active(local_identity, cx) {
                                    w.prevent_default();
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .capture_key_down(cx.listener(
                            move |this, _: &gpui_kit::KeyDownEvent, w, cx| {
                                if !this.local_toolbar_active(local_identity, cx) {
                                    w.prevent_default();
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .capture_key_up(cx.listener(
                            move |this, _: &gpui_kit::KeyUpEvent, w, cx| {
                                if !this.local_toolbar_active(local_identity, cx) {
                                    w.prevent_default();
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .on_action(cx.listener(move |this, _: &FocusFilter, w, cx| {
                            if this.local_toolbar_active(local_identity, cx) {
                                this.set_pane_view(PaneSide::Local, PaneView::Files, w, cx);
                                this.browser
                                    .update(cx, |pane, cx| pane.focus_filter_input(w, cx));
                                cx.notify();
                            }
                        }))
                        .on_action(cx.listener(move |this, _: &Refresh, w, cx| {
                            if this.local_toolbar_active(local_identity, cx) {
                                this.refresh_side(PaneSide::Local, w, cx);
                                cx.notify();
                            }
                        }))
                        .on_action(cx.listener(move |this, action: &CopySelection, w, cx| {
                            if this.local_toolbar_active(local_identity, cx) {
                                if this.local_view == PaneView::Preview {
                                    this.local_preview.update(cx, |pane, cx| pane.copy_path(cx));
                                } else {
                                    this.browser
                                        .update(cx, |pane, cx| pane.copy_selection(action, w, cx));
                                }
                            }
                        }))
                        .child(self.aligned_tabs.view(
                            PaneSide::Local,
                            local_tabs.into_any_element(),
                            cx,
                        ))
                        .child(Toolbar::local(
                            self.toolbar_state(cx),
                            cx.listener(move |this, event, w, cx| {
                                if this.local_toolbar_active(local_identity, cx) {
                                    this.toolbar_event(event, w, cx);
                                }
                            }),
                        ))
                        .child(
                            self.aligned_filters.view(
                                PaneSide::Local,
                                div()
                                    .key_context("DriftLocalFilter")
                                    .on_action(cx.listener(|this, _: &FocusResults, w, cx| {
                                        this.browser.update(cx, |pane, cx| pane.focus(w, cx));
                                    }))
                                    .on_action(cx.listener(|this, _: &Cancel, w, cx| {
                                        this.browser.update(cx, |pane, cx| pane.focus(w, cx));
                                    }))
                                    .on_action(cx.listener(
                                        |this, _: &gpui_kit::component::input::Escape, w, cx| {
                                            this.browser.update(cx, |pane, cx| pane.focus(w, cx));
                                        },
                                    ))
                                    .child(header)
                                    .into_any_element(),
                                cx,
                            ),
                        )
                        .children(local_actions)
                        .into_any_element(),
                    cx,
                ),
            )
            .child(local_path)
            .child(div().flex().flex_col().flex_1().min_h_0().min_w_0().child(
                if self.local_view == PaneView::Preview {
                    self.local_preview.clone().into_any_element()
                } else {
                    self.browser.clone().into_any_element()
                },
            ))
            .child(local_footer);
        let remote_cancel = Toolbar::remote_cancel(
            self.toolbar_state(cx),
            cx.listener(move |this, event, window, cx| {
                if this.remote_toolbar_active(identity, cx) {
                    this.toolbar_event(event, window, cx);
                }
            }),
        )
        .into_any_element();
        let remote_navigation = self.remote.update(cx, |pane, cx| {
            pane.navigation_controls(Some(remote_cancel), cx)
        });
        let remote_filter = self.remote.update(cx, |pane, cx| pane.filter_header(cx));
        let connection_controls = self
            .remote
            .update(cx, |pane, cx| pane.connection_controls(cx));
        let remote_toolbar = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Back),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    if this.remote_toolbar_active(identity, cx) {
                        this.remote.update(cx, |pane, cx| {
                            pane.mouse_navigation(
                                remote_navigation_stamp,
                                gpui_kit::NavigationDirection::Back,
                                w,
                                cx,
                            )
                        });
                    }
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Forward),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    if this.remote_toolbar_active(identity, cx) {
                        this.remote.update(cx, |pane, cx| {
                            pane.mouse_navigation(
                                remote_navigation_stamp,
                                gpui_kit::NavigationDirection::Forward,
                                w,
                                cx,
                            )
                        });
                    }
                }),
            )
            .on_action(cx.listener(move |this, action: &FocusFilter, window, cx| {
                if !this.remote_toolbar_active(identity, cx) {
                    return;
                }
                this.set_pane_view(PaneSide::Remote, PaneView::Files, window, cx);
                this.remote
                    .update(cx, |remote, cx| remote.focus_filter(action, window, cx));
            }))
            .on_action(cx.listener(move |this, _: &Refresh, window, cx| {
                if !this.remote_toolbar_active(identity, cx) {
                    return;
                }
                this.refresh_side(PaneSide::Remote, window, cx);
            }))
            .on_action(
                cx.listener(move |this, action: &CopySelection, window, cx| {
                    if !this.remote_toolbar_active(identity, cx) {
                        return;
                    }
                    if this.remote_view == PaneView::Preview {
                        this.remote_preview
                            .update(cx, |pane, cx| pane.copy_path(cx));
                    } else {
                        this.remote
                            .update(cx, |remote, cx| remote.copy(action, window, cx));
                    }
                }),
            )
            .capture_any_mouse_down(cx.listener(
                move |this, _: &gpui_kit::MouseDownEvent, window, cx| {
                    if !this.remote_toolbar_active(identity, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_any_mouse_up(cx.listener(
                move |this, _: &gpui_kit::MouseUpEvent, window, cx| {
                    if !this.remote_toolbar_active(identity, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_key_down(
                cx.listener(move |this, _: &gpui_kit::KeyDownEvent, window, cx| {
                    if !this.remote_toolbar_active(identity, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_up(
                cx.listener(move |this, _: &gpui_kit::KeyUpEvent, window, cx| {
                    if !this.remote_toolbar_active(identity, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                self.aligned_tabs.view(
                    PaneSide::Remote,
                    remote_tabs
                        .with_trailing(connection_controls)
                        .into_any_element(),
                    cx,
                ),
            )
            .child(remote_navigation)
            .child(
                self.aligned_filters
                    .view(PaneSide::Remote, remote_filter, cx),
            )
            .children(remote_preview_controls);
        let remote_workspace = div()
            .id("remote-workspace")
            .track_focus(&self.remote_focus)
            .test_support()
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Back),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    if this.remote_toolbar_active(identity, cx) {
                        this.remote.update(cx, |pane, cx| {
                            pane.mouse_navigation(
                                remote_navigation_stamp,
                                gpui_kit::NavigationDirection::Back,
                                w,
                                cx,
                            )
                        });
                    }
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Forward),
                cx.listener(move |this, _, w, cx| {
                    cx.stop_propagation();
                    if this.remote_toolbar_active(identity, cx) {
                        this.remote.update(cx, |pane, cx| {
                            pane.mouse_navigation(
                                remote_navigation_stamp,
                                gpui_kit::NavigationDirection::Forward,
                                w,
                                cx,
                            )
                        });
                    }
                }),
            )
            .capture_any_mouse_down(cx.listener(
                move |this, _: &gpui_kit::MouseDownEvent, w, cx| {
                    if this.remote_toolbar_active(identity, cx) {
                        this.active_pane = PaneSide::Remote;
                    } else {
                        w.prevent_default();
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_any_mouse_up(cx.listener(move |this, _: &gpui_kit::MouseUpEvent, w, cx| {
                if !this.remote_toolbar_active(identity, cx) {
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(self.aligned_headers.view(
                PaneSide::Remote,
                remote_toolbar.into_any_element(),
                cx,
            ))
            .child(remote_path)
            .child(div().flex().flex_col().flex_1().min_w_0().min_h_0().child(
                if self.remote_view == PaneView::Files {
                    self.remote.clone().into_any_element()
                } else {
                    self.remote_preview.clone().into_any_element()
                },
            ))
            .child(remote_footer);
        div()
            .key_context("Drift")
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .capture_action(cx.listener(|this, _: &Cancel, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .capture_action(cx.listener(|this, _: &ClearMarks, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, w, cx| {
                    this.split.cancel_resize(w, cx);
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::component::input::Escape, w, cx| {
                    this.split.cancel_resize(w, cx);
                }),
            )
            .capture_action(cx.listener(|this, _: &OpenContextMenu, _, cx| {
                if this.split.is_resizing() {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_any_mouse_down(
                cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, cx| {
                    if event.button == gpui_kit::MouseButton::Right && this.split.is_resizing() {
                        cx.stop_propagation();
                    }
                }),
            )
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::compare_marked))
            .on_action(cx.listener(Self::toggle_preview))
            .on_action(cx.listener(Self::focus_preview))
            .on_action(cx.listener(Self::copy_preview))
            .on_action(cx.listener(Self::cancel_work))
            .on_action(cx.listener(|this, _: &ResizePaneLeft, w, cx| {
                this.split.command(SplitCommand::Left, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResizePaneRight, w, cx| {
                this.split.command(SplitCommand::Right, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResetPaneSizes, w, cx| {
                this.split.command(SplitCommand::Reset, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SwitchPane, w, cx| {
                let side = match this.focused_side(w, cx) {
                    PaneSide::Local => PaneSide::Remote,
                    PaneSide::Remote => PaneSide::Local,
                };
                this.set_pane_view(side, this.pane_view(side), w, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowRemote, w, cx| {
                this.set_pane_view(PaneSide::Remote, PaneView::Files, w, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowProjects, w, cx| {
                this.toolbar_event(&ToolbarEvent::Projects, w, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowHosts, w, cx| {
                this.toolbar_event(&ToolbarEvent::Hosts, w, cx)
            }))
            .on_action(cx.listener(|this, _: &ReturnFromFinder, w, cx| {
                if this.browser.read(cx).finder_active() {
                    this.toolbar_event(&ToolbarEvent::Browser(BrowserCommand::ReturnFinder), w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FindFiles, w, cx| {
                this.set_pane_view(PaneSide::Local, PaneView::Files, w, cx);
                this.toolbar_event(&ToolbarEvent::Browser(BrowserCommand::Find), w, cx);
            }))
            .on_action(cx.listener(Self::cancel))
            .child(div().flex_1().min_h_0().min_w_0().child(self.split.view(
                "browser-split",
                local_workspace.into_any_element(),
                remote_workspace.into_any_element(),
                window,
                cx,
            )))
            .child(self.workspace_footer(identity, cx))
            .into_any_element()
    }

    /// Status on the left, compare scopes centred: comparing spans both panes.
    fn workspace_footer(
        &self,
        identity: (u64, u64, u64),
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let palette = Palette::current(cx);
        let guard = move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            if !this.remote_toolbar_active(identity, cx) {
                window.prevent_default();
                cx.stop_propagation();
            }
        };
        div()
            .id("workspace-footer")
            .test_support()
            .flex()
            .flex_wrap()
            .items_center()
            .flex_shrink_0()
            .min_w_0()
            .min_h(metric(crate::design::size::HEADER, cx))
            .gap(metric(space::CONTROL, cx))
            .px(metric(space::PANEL, cx))
            .py(metric(space::TIGHT, cx))
            .bg(palette.chrome)
            .border_t_1()
            .border_color(palette.border)
            .capture_any_mouse_down(
                cx.listener(move |this, _: &gpui_kit::MouseDownEvent, w, cx| guard(this, w, cx)),
            )
            .capture_any_mouse_up(
                cx.listener(move |this, _: &gpui_kit::MouseUpEvent, w, cx| guard(this, w, cx)),
            )
            .capture_key_down(
                cx.listener(move |this, _: &gpui_kit::KeyDownEvent, w, cx| guard(this, w, cx)),
            )
            .capture_key_up(
                cx.listener(move |this, _: &gpui_kit::KeyUpEvent, w, cx| guard(this, w, cx)),
            )
            .child(
                div()
                    .id("workspace-status")
                    .test_support()
                    .flex_1()
                    .min_w(metric(crate::design::size::FILTER / 2., cx))
                    .truncate()
                    .text_size(Text::Metadata.rems())
                    .child(self.status.clone()),
            )
            .child(Toolbar::compare(
                self.toolbar_state(cx),
                cx.listener(move |this, event, window, cx| {
                    if this.remote_toolbar_active(identity, cx) {
                        this.toolbar_event(event, window, cx);
                    }
                }),
            ))
            .child(div().flex_1().min_w_0())
            .into_any_element()
    }
}

mod aligned_header;
mod certificates;
mod comparison;
#[cfg(test)]
mod comparison_tests;
#[cfg(test)]
mod design_tests;
#[cfg(test)]
mod finder_tests;
#[cfg(test)]
mod ftp_tests;
#[cfg(test)]
mod ftps_tests;
mod keyboard;
#[cfg(test)]
mod keyboard_focus_tests;
#[cfg(test)]
mod keyboard_tests;
#[cfg(test)]
mod logging_tests;
#[cfg(test)]
mod menu_tests;
mod panes;
mod preferences;
#[cfg(test)]
mod project_tests;
mod projects;
#[cfg(test)]
mod remote_visibility_tests;
#[cfg(test)]
mod resize_focus_tests;
#[cfg(test)]
mod resize_tests;
#[cfg(test)]
mod serverlink_tests;
mod settings;
#[cfg(test)]
mod shutdown_logging_tests;
#[cfg(test)]
mod sync_tests;
#[cfg(test)]
mod tests;
