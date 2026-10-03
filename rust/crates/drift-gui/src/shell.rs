//! Window/session coordination. Child views own their interaction and load state.
use crate::certificates::CertificatePrompt;
use crate::{
    actions::*,
    browser::{BrowserCommand, BrowserEvent, BrowserPane},
    comparison::{ComparisonEvent, ComparisonPane},
    hosts::{HostEvent, HostManager},
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
    certificate: Option<(u64, u64, Entity<CertificatePrompt>)>,
    certificate_subscription: Option<Subscription>,
    certificate_cancel: Option<CancellationToken>,
    comparison: Entity<ComparisonPane>,
    browser: Entity<BrowserPane>,
    preview: Entity<PreviewPane>,
    projects: Entity<ProjectsPanel>,
    remote: Entity<RemotePane>,
    show_remote: bool,
    remote_preview: bool,
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
        let browser = cx
            .new(|cx| BrowserPane::new(store.clone(), service.clone(), start.clone(), window, cx));
        let preview = cx.new(|cx| PreviewPane::new(service.clone(), window, cx));
        let projects = cx.new(|cx| ProjectsPanel::new(store.clone(), service.clone(), window, cx));
        let remote = cx.new(|cx| RemotePane::new(remote_service.clone(), window, cx));
        let comparison = cx.new(|cx| ComparisonPane::new(service.clone(), window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&comparison, window, |this, _, event, window, cx| {
                match event {
                    ComparisonEvent::FilesChanged {
                        project,
                        connection,
                    } if *project == this.browser.read(cx).id().project => {
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
                    ComparisonEvent::Closed => this
                        .browser
                        .update(cx, |browser, cx| browser.focus(window, cx)),
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
                        if this.remote_preview || this.preview.read(cx).is_loading_remote() {
                            this.remote_preview = false;
                            this.preview.update(cx, |preview, cx| {
                                preview.select(None, *project, window, cx)
                            });
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
                            || !this.show_remote
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
                            || !this.show_remote
                            || !this.browser_screen_active(cx)
                        {
                            return;
                        }
                        this.toolbar_event(event, window, cx);
                    }
                    RemoteEvent::ShowLocalPreview => {
                        this.show_remote = false;
                        this.remote_preview = false;
                        this.preview.update(cx, |preview, cx| {
                            preview.select(None, this.browser.read(cx).id().project, window, cx)
                        });
                        this.browser
                            .update(cx, |browser, cx| browser.focus(window, cx));
                    }
                    RemoteEvent::Preview {
                        session,
                        path,
                        project,
                    } => {
                        if *project != this.browser.read(cx).id().project
                            || Some(session.id) != this.remote.read(cx).session_id()
                            || this.remote.read(cx).selected() != Some(path.as_str())
                        {
                            return;
                        }
                        this.show_remote = true;
                        this.remote_preview = true;
                        this.preview.update(cx, |preview, cx| {
                            preview.show_remote(session.clone(), path.clone(), *project, window, cx)
                        });
                        this.status = "Loading remote preview…".into();
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(&browser, window, |this, _, event, window, cx| {
                this.browser_event(event, window, cx)
            }),
            cx.subscribe_in(&preview, window, |this, _, event, _, cx| {
                match event {
                    PreviewEvent::Loaded(id)
                        if *id == this.preview.read(cx).id()
                            && id.project == this.browser.read(cx).id().project =>
                    {
                        this.status = if this.remote_preview {
                            "Remote preview loaded".into()
                        } else {
                            format!("{} entries", this.browser.read(cx).len())
                        }
                    }
                    PreviewEvent::Failed(id, error)
                        if *id == this.preview.read(cx).id()
                            && id.project == this.browser.read(cx).id().project =>
                    {
                        this.status = error.clone()
                    }
                    PreviewEvent::Copied(path) => this.status = format!("Copied {path}"),
                    _ => return,
                }
                cx.notify();
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
            certificate: None,
            certificate_subscription: None,
            certificate_cancel: None,
            comparison,
            browser,
            preview,
            projects,
            remote,
            show_remote: false,
            remote_preview: false,
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
                    self.remote_preview = false;
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
                    remote.set_selection_location(self.browser.read(cx).location().cloned(), cx);
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
                    self.show_remote = false;
                    self.remote_preview = false;
                    self.preview.update(cx, |preview, cx| {
                        preview.show(location, path.clone(), id.project, window, cx)
                    });
                    self.status = "Loading preview…".into();
                } else {
                    self.preview.update(cx, |preview, cx| {
                        preview.select(path.clone(), id.project, window, cx)
                    });
                }
            }
            BrowserEvent::Compare {
                path, connection, ..
            } => {
                if Some(*connection) != self.remote.read(cx).session_id()
                    || self.remote_preview
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
                    || self.remote_preview
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
            self.browser
                .update(cx, |browser, cx| browser.focus(window, cx));
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
                        HostEvent::Close => {
                            this.hosts = None;
                            this.host_subscription = None;
                            this.browser
                                .update(cx, |browser, cx| browser.focus(window, cx));
                        }
                        HostEvent::Changed => this.refresh_config(window, cx),
                    }
                    cx.notify();
                }),
            );
        self.hosts = Some(hosts);
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
            let _ = this.update_in(cx, |this, _, cx| {
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
        if self.remote_preview {
            self.remote.update(cx, |remote, cx| {
                remote.focus_filter(&FocusFilter, window, cx)
            });
            return;
        }
        self.browser
            .update(cx, |browser, cx| browser.focus_filter_input(window, cx));
    }
    fn copy_selection(
        &mut self,
        action: &CopySelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.remote_preview {
            self.remote
                .update(cx, |remote, cx| remote.copy(action, window, cx));
            return;
        }
        self.browser
            .update(cx, |browser, cx| browser.copy_selection(action, window, cx));
    }
    fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_preview {
            self.remote
                .update(cx, |remote, cx| remote.refresh(&Refresh, window, cx));
            return;
        }
        self.browser.update(cx, |browser, cx| {
            browser.command(BrowserCommand::Refresh, window, cx)
        });
    }
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.read(cx).visible() {
            self.projects
                .update(cx, |projects, cx| projects.cancel_view(&Cancel, window, cx));
        } else {
            self.cancel_registration(cx);
            if self.remote.read(cx).is_loading() || self.preview.read(cx).is_loading_remote() {
                self.remote.update(cx, |remote, cx| remote.cancel(cx));
            }
            self.browser.update(cx, |browser, cx| {
                browser.command(BrowserCommand::Cancel, window, cx)
            });
            self.preview.update(cx, |preview, cx| {
                preview.select(None, self.browser.read(cx).id().project, window, cx)
            });
        }
        cx.notify();
    }
    fn toolbar_event(&mut self, event: &ToolbarEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            ToolbarEvent::Remote => {
                self.show_remote = true;
                self.remote_preview = false;
                self.remote.focus_handle(cx).focus(window, cx);
            }
            ToolbarEvent::LocalBrowser => {
                self.remote_preview = false;
                self.browser
                    .update(cx, |browser, cx| browser.focus(window, cx));
            }
            ToolbarEvent::Projects => {
                self.projects
                    .update(cx, |projects, cx| projects.toggle(window, cx));
                if !self.projects.read(cx).visible() {
                    self.close_projects(window, cx);
                }
            }
            ToolbarEvent::Hosts => self.open_hosts(window, cx),
            ToolbarEvent::OpenFolder => self.open_folder(window, cx),
            ToolbarEvent::Cancel => self.cancel(&Cancel, window, cx),
            ToolbarEvent::CompareProject
            | ToolbarEvent::CompareLocal
            | ToolbarEvent::CompareRemote
            | ToolbarEvent::CompareMarked => self.compare(event, window, cx),
            ToolbarEvent::Browser(command) => {
                self.remote_preview = false;
                self.browser
                    .update(cx, |browser, cx| browser.command(*command, window, cx));
            }
        }
        cx.notify();
    }
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(window, cx);
        let mut view = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground);
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
        view.child(div().flex().flex_col().flex_1().min_h_0().child(content))
    }
}
impl Shell {
    fn browser_screen_active(&self, cx: &gpui_kit::App) -> bool {
        self.certificate.is_none()
            && self.hosts.is_none()
            && self.startup.is_none()
            && !self.projects.read(cx).visible()
            && !self.comparison.read(cx).visible()
    }

    fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let browser_active = self.browser_screen_active(cx);
        let can_compare = self.remote.read(cx).has_session()
            && !self.remote.read(cx).is_loading()
            && !self.browser.read(cx).is_loading();
        let comparison_ready =
            browser_active && can_compare && self.browser.read(cx).location().is_some();
        let local_visible = browser_active && !self.remote_preview;
        let remote_visible = browser_active && self.show_remote;
        let comparison_connection = if comparison_ready && local_visible {
            self.remote.read(cx).session_id()
        } else {
            None
        };
        self.browser.update(cx, |pane, cx| {
            pane.set_comparison_context(comparison_connection, window, cx);
            if !local_visible {
                pane.dismiss_context_menu(window, cx);
            }
        });
        self.remote.update(cx, |pane, cx| {
            pane.set_comparison_ready(comparison_ready && remote_visible, window, cx);
            if !remote_visible {
                pane.dismiss_context_menu(window, cx);
            }
        });
        if let Some((_, _, prompt)) = &self.certificate {
            return div().size_full().child(prompt.clone()).into_any_element();
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
            return div()
                .flex()
                .flex_col()
                .size_full()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(self.comparison.clone())
                .child(div().p_2().child(self.status.clone()))
                .into_any_element();
        }
        let browser = self.browser.read(cx);
        let toolbar = Toolbar::new(
            ToolbarState {
                remote_preview: self.remote_preview,
                can_compare,
                has_location: browser.location().is_some(),
                listing: browser.is_loading(),
                cancellable: browser.is_loading()
                    || self.preview.read(cx).is_loading()
                    || self.registration.is_some()
                    || self.remote.read(cx).is_loading(),
                folder_prompt: self.folder_prompt,
                can_back: browser.can_back(),
                can_forward: browser.can_forward(),
                can_up: browser.can_up(),
                show_hidden: browser.shows_hidden(),
                show_ignored: browser.shows_ignored(),
            },
            cx.listener(Self::toolbar_event),
        );
        let header = browser.header();
        let hosts = browser
            .location()
            .map(|location| {
                location
                    .config
                    .hosts
                    .iter()
                    .map(|host| {
                        format!(
                            "{} ({})",
                            host.name,
                            if host.protocol.is_empty() {
                                "sftp"
                            } else {
                                &host.protocol
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        div()
            .key_context("Drift")
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::compare_marked))
            .on_action(cx.listener(|this, _: &SwitchPane, w, cx| {
                let event = if this.remote.focus_handle(cx).is_focused(w) {
                    ToolbarEvent::LocalBrowser
                } else {
                    ToolbarEvent::Remote
                };
                this.toolbar_event(&event, w, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowRemote, w, cx| {
                this.toolbar_event(&ToolbarEvent::Remote, w, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowProjects, w, cx| {
                this.toolbar_event(&ToolbarEvent::Projects, w, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowHosts, w, cx| {
                this.toolbar_event(&ToolbarEvent::Hosts, w, cx)
            }))
            .on_action(cx.listener(|this, _: &FindFiles, w, cx| {
                this.toolbar_event(&ToolbarEvent::LocalBrowser, w, cx);
                this.toolbar_event(&ToolbarEvent::Browser(BrowserCommand::Find), w, cx);
            }))
            .on_action(cx.listener(Self::cancel))
            .child(toolbar)
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(if self.remote_preview {
                        self.preview.clone().into_any_element()
                    } else {
                        self.browser.clone().into_any_element()
                    })
                    .child(if self.show_remote {
                        self.remote.clone().into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.preview.clone())
                            .child(div().p_2().child(if hosts.is_empty() {
                                "No hosts configured".into()
                            } else {
                                format!("Hosts: {hosts}")
                            }))
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(self.status.clone()),
            )
            .into_any_element()
    }
}

mod certificates;
mod comparison;
#[cfg(test)]
mod comparison_tests;
#[cfg(test)]
mod ftp_tests;
#[cfg(test)]
mod ftps_tests;
#[cfg(test)]
mod logging_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod project_tests;
mod projects;
#[cfg(test)]
mod shutdown_logging_tests;
#[cfg(test)]
mod sync_tests;
#[cfg(test)]
mod tests;
