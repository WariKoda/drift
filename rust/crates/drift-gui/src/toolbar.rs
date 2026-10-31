//! Stateless shell controls; actions are routed by the session owner.
use crate::browser::BrowserCommand;
use crate::design::{Palette, Text, control, metric, size, space};
use gpui_kit::assets::IconName;
use gpui_kit::base::{Disableable, Selectable};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::{
    App, ClickEvent, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, TestSupportExt, Window, div,
};
use std::rc::Rc;

pub enum ToolbarEvent {
    Home,
    Settings,
    Projects,
    Hosts,
    OpenFolder,
    Cancel,
    CancelRemote,
    CompareProject,
    CompareLocal,
    CompareRemote,
    CompareMarked,
    Browser(BrowserCommand),
}
#[derive(Clone, Copy)]
pub struct ToolbarState {
    pub management_enabled: bool,
    pub browser_enabled: bool,
    pub projects_open: bool,
    pub hosts_open: bool,
    pub settings_open: bool,
    pub can_compare: bool,
    pub has_location: bool,
    pub listing: bool,
    pub cancellable: bool,
    pub remote_cancellable: bool,
    pub folder_prompt: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub can_up: bool,
    pub show_hidden: bool,
    pub show_ignored: bool,
}
impl ToolbarState {
    pub fn allows(&self, event: &ToolbarEvent) -> bool {
        match event {
            ToolbarEvent::Home | ToolbarEvent::Settings => self.management_enabled,
            ToolbarEvent::Projects => self.management_enabled,
            // The open manager may close itself from the toolbar, like Projects.
            ToolbarEvent::Hosts => {
                self.management_enabled
                    && (self.hosts_open
                        || ((self.browser_enabled || self.settings_open)
                            && self.has_location
                            && !self.listing))
            }
            ToolbarEvent::OpenFolder => self.browser_enabled && !self.folder_prompt,
            ToolbarEvent::Cancel => self.browser_enabled && self.cancellable,
            ToolbarEvent::CancelRemote => self.browser_enabled && self.remote_cancellable,
            ToolbarEvent::CompareProject
            | ToolbarEvent::CompareLocal
            | ToolbarEvent::CompareRemote
            | ToolbarEvent::CompareMarked => self.browser_enabled && self.can_compare,
            ToolbarEvent::Browser(command) => {
                self.browser_enabled
                    && match command {
                        BrowserCommand::Back => self.can_back,
                        BrowserCommand::Forward => self.can_forward,
                        BrowserCommand::Up => self.can_up,
                        BrowserCommand::Find => self.has_location,
                        _ => true,
                    }
            }
        }
    }
}
type Listener = Rc<dyn Fn(&ToolbarEvent, &mut Window, &mut App)>;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Area {
    Global,
    Local,
    RemoteCancel,
    Compare,
}

#[derive(IntoElement)]
pub struct Toolbar {
    state: ToolbarState,
    listener: Listener,
    area: Area,
}
impl Toolbar {
    pub fn new(
        state: ToolbarState,
        listener: impl Fn(&ToolbarEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            state,
            listener: Rc::new(listener),
            area: Area::Global,
        }
    }
    /// Cancels remote work only; it sits at the end of the remote navigation row.
    pub fn remote_cancel(
        state: ToolbarState,
        listener: impl Fn(&ToolbarEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            area: Area::RemoteCancel,
            ..Self::new(state, listener)
        }
    }
    /// Compare scopes involve both panes, so they live below the split.
    pub fn compare(
        state: ToolbarState,
        listener: impl Fn(&ToolbarEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            area: Area::Compare,
            ..Self::new(state, listener)
        }
    }
    pub fn local(
        state: ToolbarState,
        listener: impl Fn(&ToolbarEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            area: Area::Local,
            ..Self::new(state, listener)
        }
    }
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        icon: IconName,
        disabled: bool,
        event: ToolbarEvent,
        cx: &App,
    ) -> Button {
        let listener = self.listener.clone();
        let name = match id {
            "compare-project" => "Compare project",
            "compare-local" => "Compare local selection",
            "compare-remote" => "Compare remote selection",
            "compare-marked" => "Compare marked paths",
            _ => label,
        };
        let button = Button::new(id)
            .accessibility_label(name)
            .tooltip(name)
            .icon(icon)
            .disabled(disabled || !self.state.allows(&event))
            .selected(match id {
                "projects" => self.state.projects_open,
                "hosts" => self.state.hosts_open,
                "settings" => self.state.settings_open,
                "hidden" => self.state.show_hidden,
                "ignored" => self.state.show_ignored,
                _ => false,
            })
            .on_click(move |_: &ClickEvent, window, cx| listener(&event, window, cx));
        if matches!(self.area, Area::Local | Area::RemoteCancel)
            || matches!(
                id,
                "home"
                    | "settings"
                    | "back"
                    | "forward"
                    | "up"
                    | "refresh"
                    | "find"
                    | "hidden"
                    | "ignored"
                    | "cancel"
            )
        {
            button.w(metric(size::CONTROL, cx))
        } else {
            button.label(label)
        }
    }
}
impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = &self.state;
        let palette = Palette::current(cx);
        let button = |id, label, icon, disabled, event| {
            let button = control(self.button(id, label, icon, disabled, event, cx), cx);
            if matches!(
                id,
                "compare-project" | "compare-local" | "compare-remote" | "compare-marked"
            ) {
                button.secondary()
            } else {
                button
            }
        };
        let group = |id| {
            div()
                .id(id)
                .flex()
                .flex_wrap()
                .items_center()
                .min_w_0()
                .max_w_full()
                .gap(metric(space::TIGHT, cx))
        };
        if self.area == Area::Local {
            return div()
                .id("local-navigation-toolbar")
                .test_support()
                .flex()
                .flex_shrink_0()
                .items_center()
                .min_w_0()
                .h(metric(size::HEADER, cx))
                .overflow_x_scroll()
                .gap(metric(space::TIGHT, cx))
                .px(metric(space::PANEL, cx))
                .py(metric(space::TIGHT, cx))
                .bg(palette.chrome)
                .border_b_1()
                .border_color(palette.border)
                .children(
                    [
                        (
                            "back",
                            "Back",
                            IconName::ArrowLeft,
                            ToolbarEvent::Browser(BrowserCommand::Back),
                        ),
                        (
                            "forward",
                            "Forward",
                            IconName::ArrowRight,
                            ToolbarEvent::Browser(BrowserCommand::Forward),
                        ),
                        (
                            "up",
                            "Up",
                            IconName::ArrowUp,
                            ToolbarEvent::Browser(BrowserCommand::Up),
                        ),
                        (
                            "refresh",
                            "Refresh",
                            IconName::RefreshCw,
                            ToolbarEvent::Browser(BrowserCommand::Refresh),
                        ),
                        (
                            "find",
                            "Find files",
                            IconName::Search,
                            ToolbarEvent::Browser(BrowserCommand::Find),
                        ),
                        (
                            "hidden",
                            if state.show_hidden {
                                "Hide hidden"
                            } else {
                                "Show hidden"
                            },
                            IconName::Eye,
                            ToolbarEvent::Browser(BrowserCommand::Hidden),
                        ),
                        (
                            "ignored",
                            if state.show_ignored {
                                "Hide ignored"
                            } else {
                                "Show ignored"
                            },
                            IconName::EyeOff,
                            ToolbarEvent::Browser(BrowserCommand::Ignored),
                        ),
                        ("cancel", "Cancel", IconName::Close, ToolbarEvent::Cancel),
                    ]
                    .into_iter()
                    .map(|(id, label, icon, event)| {
                        let button = button(id, label, icon, false, event).flex_shrink_0();
                        if id == "cancel" {
                            button.tooltip("Cancel local operation")
                        } else {
                            button
                        }
                    }),
                );
        }
        if self.area == Area::RemoteCancel {
            return div()
                .id("remote-cancel-slot")
                .test_support()
                .flex_shrink_0()
                .child(
                    button(
                        "remote-cancel",
                        "Cancel remote operation",
                        IconName::Close,
                        false,
                        ToolbarEvent::CancelRemote,
                    )
                    .flex_shrink_0(),
                );
        }
        if self.area == Area::Compare {
            return div()
                .id("workspace-compare")
                .test_support()
                .flex()
                .flex_wrap()
                .justify_center()
                .items_center()
                .min_w_0()
                .gap(metric(space::TIGHT, cx))
                .text_size(Text::Body.rems())
                .child(
                    div()
                        .text_size(Text::Metadata.rems())
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child("Compare"),
                )
                .child(button(
                    "compare-project",
                    "Project",
                    IconName::Folder,
                    false,
                    ToolbarEvent::CompareProject,
                ))
                .child(button(
                    "compare-local",
                    "Local selection",
                    IconName::HardDrive,
                    false,
                    ToolbarEvent::CompareLocal,
                ))
                .child(button(
                    "compare-remote",
                    "Remote selection",
                    IconName::Globe,
                    false,
                    ToolbarEvent::CompareRemote,
                ))
                .child(button(
                    "compare-marked",
                    "Marked",
                    IconName::Check,
                    false,
                    ToolbarEvent::CompareMarked,
                ));
        }
        div()
            .id("workspace-toolbar")
            .test_support()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .text_size(Text::Body.rems())
            .bg(palette.chrome)
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .id("workspace-tasks")
                    .test_support()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .min_w_0()
                    .min_h(metric(size::HEADER, cx))
                    .gap(metric(space::CONTROL, cx))
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .child(
                        button("home", "Home", IconName::House, false, ToolbarEvent::Home)
                            .tooltip("Back to browser"),
                    )
                    .child(
                        group("toolbar-project")
                            .child(button(
                                "projects",
                                "Projects",
                                IconName::Folder,
                                false,
                                ToolbarEvent::Projects,
                            ))
                            .child(button(
                                "hosts",
                                "Hosts",
                                IconName::Network,
                                false,
                                ToolbarEvent::Hosts,
                            ))
                            .child(button(
                                "open-folder",
                                "Open folder",
                                IconName::FolderOpen,
                                state.folder_prompt,
                                ToolbarEvent::OpenFolder,
                            )),
                    )
                    .child(button(
                        "settings",
                        "Settings",
                        IconName::Settings,
                        false,
                        ToolbarEvent::Settings,
                    )),
            )
    }
}
