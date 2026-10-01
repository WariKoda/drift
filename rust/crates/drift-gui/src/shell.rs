//! Window/session coordination. Child views own their interaction and load state.
use crate::{
    actions::*,
    browser::{BrowserCommand, BrowserEvent, BrowserPane},
    hosts::{HostEvent, HostManager},
    preview::{PreviewEvent, PreviewPane},
    projects::{ProjectEvent, ProjectsPanel},
    toolbar::{Toolbar, ToolbarEvent, ToolbarState},
};
use drift_app::{
    browser::{BrowserService, OperationId},
    hosts::{HostCommand, HostResponse},
};
use drift_core::store::Store;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, PathPromptOptions,
    Render, Styled, Subscription, Window, div,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub struct Shell {
    browser: Entity<BrowserPane>,
    preview: Entity<PreviewPane>,
    projects: Entity<ProjectsPanel>,
    hosts: Option<Entity<HostManager>>,
    store: Store,
    service: BrowserService,
    configuration: u64,
    config_cancel: Option<(OperationId, CancellationToken)>,
    registration: Option<(OperationId, CancellationToken)>,
    folder_prompt: bool,
    status: String,
    host_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}
impl Drop for Shell {
    fn drop(&mut self) {
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
        start: PathBuf,
    ) -> Self {
        let browser = cx
            .new(|cx| BrowserPane::new(store.clone(), service.clone(), start.clone(), window, cx));
        let preview = cx.new(|cx| PreviewPane::new(service.clone(), window, cx));
        let projects = cx.new(|cx| ProjectsPanel::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&browser, window, |this, _, event, window, cx| {
                this.browser_event(event, window, cx)
            }),
            cx.subscribe_in(&preview, window, |this, _, event, _, cx| {
                match event {
                    PreviewEvent::Loaded(id)
                        if *id == this.preview.read(cx).id()
                            && id.project == this.browser.read(cx).id().project =>
                    {
                        this.status = format!("{} entries", this.browser.read(cx).len())
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
                },
            ),
        ];
        let shell = Self {
            browser,
            preview,
            projects,
            hosts: None,
            store,
            service,
            configuration: 0,
            config_cancel: None,
            registration: None,
            folder_prompt: false,
            status: "Opening project…".into(),
            host_subscription: None,
            _subscriptions: subscriptions,
        };
        shell
            .browser
            .update(cx, |browser, cx| browser.open(start, window, cx));
        shell
    }
    fn browser_event(&mut self, event: &BrowserEvent, window: &mut Window, cx: &mut Context<Self>) {
        let id = match event {
            BrowserEvent::Changed(id)
            | BrowserEvent::Opened { id, .. }
            | BrowserEvent::Selected { id, .. }
            | BrowserEvent::Status { id, .. } => *id,
        };
        if id != self.browser.read(cx).id() {
            return;
        }
        match event {
            BrowserEvent::Changed(_) => {
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
            BrowserEvent::Status { message, .. } => self.status.clone_from(message),
        }
        cx.notify();
    }
    fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_registration(cx);
        self.projects.update(cx, |projects, cx| projects.close(cx));
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
        let hosts = cx
            .new(|cx| HostManager::new(self.store.clone(), self.service.clone(), slug, window, cx));
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
                        this.browser.update(cx, |browser, cx| {
                            browser.replace_config(catalog.runtime, cx)
                        })
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
        self.browser
            .update(cx, |browser, cx| browser.focus_filter_input(window, cx));
    }
    fn copy_selection(
        &mut self,
        action: &CopySelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.browser
            .update(cx, |browser, cx| browser.copy_selection(action, window, cx));
    }
    fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        self.browser.update(cx, |browser, cx| {
            browser.command(BrowserCommand::Refresh, window, cx)
        });
    }
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.read(cx).visible() {
            self.projects.update(cx, |projects, cx| projects.close(cx));
            self.browser
                .update(cx, |browser, cx| browser.focus(window, cx));
        } else {
            self.cancel_registration(cx);
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
            ToolbarEvent::Projects => {
                self.projects
                    .update(cx, |projects, cx| projects.toggle(window, cx));
                if !self.projects.read(cx).visible() {
                    self.browser
                        .update(cx, |browser, cx| browser.focus(window, cx));
                }
            }
            ToolbarEvent::Hosts => self.open_hosts(window, cx),
            ToolbarEvent::OpenFolder => self.open_folder(window, cx),
            ToolbarEvent::Cancel => self.cancel(&Cancel, window, cx),
            ToolbarEvent::Browser(command) => self
                .browser
                .update(cx, |browser, cx| browser.command(*command, window, cx)),
        }
        cx.notify();
    }
}
impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(hosts) = &self.hosts {
            return div().size_full().child(hosts.clone()).into_any_element();
        }
        let browser = self.browser.read(cx);
        let toolbar = Toolbar::new(
            ToolbarState {
                has_location: browser.location().is_some(),
                listing: browser.is_loading(),
                cancellable: browser.is_loading()
                    || self.preview.read(cx).is_loading()
                    || self.registration.is_some(),
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
            .on_action(cx.listener(Self::cancel))
            .child(toolbar)
            .child(header)
            .when(self.projects.read(cx).visible(), |view| {
                view.child(self.projects.clone())
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.browser.clone())
                    .child(
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
                            })),
                    ),
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

#[cfg(test)]
mod tests;
