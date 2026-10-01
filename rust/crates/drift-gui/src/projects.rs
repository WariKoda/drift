//! Project switcher input state. Registry I/O and project changes belong to Shell.
use drift_core::project::Registry;
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{
    AppContext, Context, Entity, EventEmitter, Focusable, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div, px,
};
use std::path::PathBuf;

pub enum ProjectEvent {
    Open(PathBuf),
    Register(String),
}
impl EventEmitter<ProjectEvent> for ProjectsPanel {}
pub struct ProjectsPanel {
    query: Entity<InputState>,
    name: Entity<InputState>,
    registry: Registry,
    visible: bool,
    can_register: bool,
    busy: bool,
    _subscription: Subscription,
}
impl ProjectsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            _subscription: subscription,
        }
    }
    pub fn visible(&self) -> bool {
        self.visible
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
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            self.query.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        cx.notify();
    }
}
impl Render for ProjectsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query.read(cx).value().to_lowercase();
        div()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(Input::new(&self.query).id("project-filter").w(px(320.)))
            .children(
                self.registry
                    .active()
                    .into_iter()
                    .filter(|project| {
                        project.name.to_lowercase().contains(&query)
                            || project.slug.to_lowercase().contains(&query)
                    })
                    .map(|project| {
                        let path = project.path.clone();
                        Button::new(project.slug.clone())
                            .label(project.name.clone())
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(ProjectEvent::Open(path.clone()))
                            }))
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Input::new(&self.name).id("project-name").w(px(280.)))
                    .child(
                        Button::new("register")
                            .label("Register current folder")
                            .disabled(!self.can_register || self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(ProjectEvent::Register(
                                    this.name.read(cx).value().to_string(),
                                ))
                            })),
                    ),
            )
    }
}
