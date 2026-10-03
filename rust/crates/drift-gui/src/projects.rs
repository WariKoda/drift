//! Project dashboard/switcher. Store mutations run in the application service.
mod form;
mod keyboard;
mod management;
use crate::actions::Cancel;
use crate::actions::{CursorDown, CursorFirst, CursorLast, CursorUp};
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
use gpui_kit::prelude::FluentBuilder;
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, Window, div, px,
};
pub use keyboard::bind_keys;
use keyboard::*;
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
    list_focus: FocusHandle,
    cursor: Option<String>,
    scroll: ScrollHandle,
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
        let subscription = cx.subscribe_in(&query, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.scroll.scroll_to_item(this.cursor_row(cx));
            }
            cx.notify();
        });
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
            list_focus: cx.focus_handle().tab_stop(true),
            cursor: None,
            scroll: ScrollHandle::new(),
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
            self.list_focus.focus(window, cx);
            cx.notify();
        } else {
            self.close(cx);
            cx.emit(ProjectEvent::Close);
        }
    }
}
impl Render for ProjectsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected_project(cx).map(|p| p.slug);
        let busy = self.is_loading();
        let editing = self.form.is_some() || self.delete.is_some();
        let mut view = div()
            .key_context(if self.form.is_some() {
                "Drift DriftProjects DriftProjectForm"
            } else if self.delete.is_some() {
                "Drift DriftProjects DriftProjectConfirm"
            } else {
                "Drift DriftProjects"
            })
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::cancel_view))
            .on_action(cx.listener(|this, _: &Search, w, cx| {
                if this.form.is_none() && this.delete.is_none() {
                    this.query.focus_handle(cx).focus(w, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &FocusList, w, cx| {
                    this.select_row(this.cursor_row(cx), w, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Save, w, cx| this.save_form(w, cx)))
            .on_action(cx.listener(|this, _: &Confirm, w, cx| this.confirm_delete(w, cx)))
            .child(
                "Projects · ↓ from filter · j/k navigate · n new · e edit · a archive · d remove",
            )
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
                        .label("Save project (Ctrl/Cmd+S)")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, w, cx| this.save_form(w, cx))),
                );
        } else if let Some(project) = &self.delete {
            view = view.child(format!("Remove {:?} and its saved hosts/mappings? Local project files remain in place.", project.name))
                .child(Button::new("project-delete-confirm").label("Remove project").disabled(busy).on_click(cx.listener(|this, _, w, cx| {
                    this.confirm_delete(w, cx);
                })));
        } else {
            view = view
                .child(
                    div()
                        .key_context("DriftProjectFilter")
                        .child(Input::new(&self.query).id("project-filter").w(px(320.))),
                )
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
                                    this.toggle_archived(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .id("project-list")
                        .key_context("DriftProjectList")
                        .track_focus(&self.list_focus)
                        .track_scroll(&self.scroll)
                        .on_action(cx.listener(|this, _: &CursorUp, w, cx| {
                            this.select_row(this.cursor_row(cx).saturating_sub(1), w, cx)
                        }))
                        .on_action(cx.listener(|this, _: &CursorDown, w, cx| {
                            this.select_row(this.cursor_row(cx) + 1, w, cx)
                        }))
                        .on_action(
                            cx.listener(|this, _: &CursorFirst, w, cx| this.select_row(0, w, cx)),
                        )
                        .on_action(cx.listener(|this, _: &CursorLast, w, cx| {
                            this.select_row(usize::MAX, w, cx)
                        }))
                        .on_action(cx.listener(|this, _: &New, w, cx| {
                            if !this.is_loading() {
                                this.edit(None, w, cx);
                            }
                        }))
                        .on_action(cx.listener(|this, _: &Edit, w, cx| {
                            if !this.is_loading()
                                && let Some(project) = this.selected_project(cx)
                            {
                                this.edit(Some(project), w, cx);
                            }
                        }))
                        .on_action(cx.listener(|this, _: &Open, _, cx| {
                            this.open_row(this.cursor_row(cx), cx);
                        }))
                        .on_action(cx.listener(|this, _: &Open1, _, cx| this.open_row(0, cx)))
                        .on_action(cx.listener(|this, _: &Open2, _, cx| this.open_row(1, cx)))
                        .on_action(cx.listener(|this, _: &Open3, _, cx| this.open_row(2, cx)))
                        .on_action(cx.listener(|this, _: &Open4, _, cx| this.open_row(3, cx)))
                        .on_action(cx.listener(|this, _: &Open5, _, cx| this.open_row(4, cx)))
                        .on_action(cx.listener(|this, _: &Open6, _, cx| this.open_row(5, cx)))
                        .on_action(cx.listener(|this, _: &Open7, _, cx| this.open_row(6, cx)))
                        .on_action(cx.listener(|this, _: &Open8, _, cx| this.open_row(7, cx)))
                        .on_action(cx.listener(|this, _: &Open9, _, cx| this.open_row(8, cx)))
                        .on_action(cx.listener(|this, _: &Remove, w, cx| {
                            if !this.is_loading()
                                && let Some(project) = this.selected_project(cx)
                            {
                                this.cursor = Some(project.slug.clone());
                                this.delete = Some(project);
                                this.focus.focus(w, cx);
                                cx.notify();
                            }
                        }))
                        .on_action(cx.listener(|this, _: &Archive, w, cx| {
                            if !this.is_loading()
                                && let Some(project) = this.selected_project(cx)
                            {
                                this.request(
                                    ProjectCommand::Archive {
                                        expected: Box::new(project),
                                    },
                                    w,
                                    cx,
                                );
                            }
                        }))
                        .on_action(cx.listener(|this, _: &Archived, _, cx| {
                            if !this.is_loading() {
                                this.toggle_archived(cx);
                            }
                        }))
                        .on_action(cx.listener(|this, _: &Reload, w, cx| {
                            if !this.is_loading() {
                                this.request(ProjectCommand::Load, w, cx);
                            }
                        }))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(self.visible_projects(cx).into_iter().enumerate().map(
                            |(index, project)| {
                                let path = project.path.clone();
                                let edit = project.clone();
                                let archive = project.clone();
                                let delete = project.clone();
                                div()
                                    .id(format!("project-row-{}", project.slug))
                                    .when(selected.as_ref() == Some(&project.slug), |view| {
                                        view.bg(cx.theme().accent)
                                    })
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.select_row(index, w, cx)
                                    }))
                                    .map(|view| {
                                        #[cfg(test)]
                                        {
                                            view.test_support()
                                        }
                                        #[cfg(not(test))]
                                        {
                                            view
                                        }
                                    })
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(format!(
                                        "{}{}{} — {}",
                                        if index < 9 {
                                            format!("[{}] ", index + 1)
                                        } else {
                                            String::new()
                                        },
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
                                                    this.cursor = Some(delete.slug.clone());
                                                    this.delete = Some(delete.clone());
                                                    this.focus.focus(w, cx);
                                                    cx.notify();
                                                })),
                                            ),
                                    )
                            },
                        )),
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
