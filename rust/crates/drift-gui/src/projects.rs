//! Project dashboard/switcher. Store mutations run in the application service.
mod form;
mod management;
use crate::actions::Cancel;
use drift_app::{
    browser::{BrowserService, OperationId},
    projects::{ProjectCommand, ProjectResponse},
};
use drift_core::{
    project::{Project, Registry},
    store::Store,
};
use form::ProjectForm;
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
    div, px,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub enum ProjectEvent {
    Open(PathBuf),
    Register(String),
    Changed(Box<ProjectResponse>),
    Close,
}
impl EventEmitter<ProjectEvent> for ProjectsPanel {}
pub struct ProjectsPanel {
    query: Entity<InputState>,
    name: Entity<InputState>,
    registry: Registry,
    visible: bool,
    can_register: bool,
    busy: bool,
    store: Store,
    service: BrowserService,
    operation: u64,
    cancel: Option<CancellationToken>,
    writing: bool,
    show_archived: bool,
    form: Option<ProjectForm>,
    delete: Option<Project>,
    focus: FocusHandle,
    status: String,
    _subscription: Subscription,
}
impl Drop for ProjectsPanel {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
    }
}
impl ProjectsPanel {
    pub fn new(
        store: Store,
        service: BrowserService,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find a project…"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        let subscription =
            cx.subscribe_in(&query, window, |_, _, _: &InputEvent, _, cx| cx.notify());
        Self {
            query,
            name,
            registry: Registry::default(),
            visible: false,
            can_register: false,
            busy: false,
            store,
            service,
            operation: 0,
            cancel: None,
            writing: false,
            show_archived: false,
            form: None,
            delete: None,
            focus: cx.focus_handle(),
            status: String::new(),
            _subscription: subscription,
        }
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn is_loading(&self) -> bool {
        self.busy || self.cancel.is_some()
    }
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.focus_handle(cx).focus(window, cx);
    }
    pub fn set_context(&mut self, registry: Registry, can_register: bool, cx: &mut Context<Self>) {
        self.registry = registry;
        self.can_register = can_register;
        cx.notify();
    }
    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        self.query.focus_handle(cx).focus(window, cx);
        self.request(ProjectCommand::Load, window, cx);
    }
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible {
            self.close(cx);
        } else {
            self.show(window, cx);
        }
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.writing {
            return;
        }
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.operation += 1;
        self.form = None;
        self.delete = None;
        self.visible = false;
        cx.notify();
    }
    pub(super) fn cancel_view(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.writing {
            return;
        }
        if self.form.take().is_some() || self.delete.take().is_some() {
            self.query.focus_handle(cx).focus(window, cx);
            cx.notify();
        } else {
            self.close(cx);
            cx.emit(ProjectEvent::Close);
        }
    }
}
impl Render for ProjectsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query.read(cx).value().to_lowercase();
        let busy = self.is_loading();
        let editing = self.form.is_some() || self.delete.is_some();
        let mut view = div()
            .key_context("Drift")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::cancel_view))
            .child("Projects")
            .child(self.status.clone());
        if let Some(form) = &self.form {
            view = view
                .child(
                    Input::new(&form.name)
                        .id("project-edit-name")
                        .disabled(busy),
                )
                .child(
                    Input::new(&form.path)
                        .id("project-edit-path")
                        .disabled(busy),
                )
                .child(
                    Button::new("project-save")
                        .label("Save project")
                        .disabled(busy)
                        .on_click(cx.listener(Self::save_form)),
                );
        } else if let Some(project) = &self.delete {
            view = view.child(format!("Remove {:?} and its saved hosts/mappings? Local project files remain in place.", project.name))
                .child(Button::new("project-delete-confirm").label("Remove project").disabled(busy).on_click(cx.listener(|this, _, w, cx| {
                    if let Some(expected) = this.delete.clone() { this.request(ProjectCommand::Remove { expected: Box::new(expected) }, w, cx); }
                })));
        } else {
            view = view
                .child(Input::new(&self.query).id("project-filter").w(px(320.)))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("project-new")
                                .label("New project")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, w, cx| this.edit(None, w, cx))),
                        )
                        .child(
                            Button::new("project-reload")
                                .label("Reload")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, w, cx| {
                                    this.request(ProjectCommand::Load, w, cx)
                                })),
                        )
                        .child(
                            Button::new("project-archived")
                                .label(if self.show_archived {
                                    "Hide archived"
                                } else {
                                    "Show archived"
                                })
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_archived = !this.show_archived;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("project-list")
                        .flex()
                        .flex_col()
                        .gap_2()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(
                            (if self.show_archived {
                                self.registry.all()
                            } else {
                                self.registry.active()
                            })
                            .into_iter()
                            .filter(|p| {
                                p.name.to_lowercase().contains(&query)
                                    || p.slug.to_lowercase().contains(&query)
                                    || p.path.to_string_lossy().to_lowercase().contains(&query)
                            })
                            .map(|project| {
                                let path = project.path.clone();
                                let edit = project.clone();
                                let archive = project.clone();
                                let delete = project.clone();
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(format!(
                                        "{}{} — {}",
                                        project.name,
                                        if project.archived { " (archived)" } else { "" },
                                        project.path.display()
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                Button::new(format!(
                                                    "project-open-{}",
                                                    project.slug
                                                ))
                                                .label("Open")
                                                .disabled(busy)
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    cx.emit(ProjectEvent::Open(path.clone()))
                                                })),
                                            )
                                            .child(
                                                Button::new(format!(
                                                    "project-edit-{}",
                                                    project.slug
                                                ))
                                                .label("Edit")
                                                .disabled(busy)
                                                .on_click(cx.listener(move |this, _, w, cx| {
                                                    this.edit(Some(edit.clone()), w, cx)
                                                })),
                                            )
                                            .child(
                                                Button::new(format!(
                                                    "project-archive-{}",
                                                    project.slug
                                                ))
                                                .label(if project.archived {
                                                    "Unarchive"
                                                } else {
                                                    "Archive"
                                                })
                                                .disabled(busy)
                                                .on_click(cx.listener(move |this, _, w, cx| {
                                                    this.request(
                                                        ProjectCommand::Archive {
                                                            expected: Box::new(archive.clone()),
                                                        },
                                                        w,
                                                        cx,
                                                    )
                                                })),
                                            )
                                            .child(
                                                Button::new(format!(
                                                    "project-delete-{}",
                                                    project.slug
                                                ))
                                                .label("Remove")
                                                .disabled(busy)
                                                .on_click(cx.listener(move |this, _, w, cx| {
                                                    this.delete = Some(delete.clone());
                                                    this.focus.focus(w, cx);
                                                    cx.notify();
                                                })),
                                            ),
                                    )
                            }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Input::new(&self.name).id("project-name").w(px(280.)))
                        .child(
                            Button::new("register")
                                .label("Register current folder")
                                .disabled(!self.can_register || self.busy || self.writing)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.emit(ProjectEvent::Register(
                                        this.name.read(cx).value().to_string(),
                                    ))
                                })),
                        ),
                );
        }
        view.child(
            Button::new("project-close")
                .label(if editing {
                    "Cancel"
                } else {
                    "Return to browser"
                })
                .disabled(self.writing || self.busy)
                .on_click(cx.listener(|this, _, w, cx| this.cancel_view(&Cancel, w, cx))),
        )
    }
}
#[cfg(test)]
mod tests;
