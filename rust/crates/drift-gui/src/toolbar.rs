//! Stateless shell controls; actions are routed by the session owner.
use crate::browser::BrowserCommand;
use crate::design::{Palette, Text, control, metric, size, space};
use gpui_kit::assets::IconName;
use gpui_kit::base::Disableable;
use gpui_kit::component::button::Button;
use gpui_kit::{
    App, ClickEvent, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use std::rc::Rc;

pub enum ToolbarEvent {
    Projects,
    Remote,
    LocalBrowser,
    Hosts,
    OpenFolder,
    Cancel,
    CompareProject,
    CompareLocal,
    CompareRemote,
    CompareMarked,
    Browser(BrowserCommand),
}
pub struct ToolbarState {
    pub remote_preview: bool,
    pub can_compare: bool,
    pub has_location: bool,
    pub listing: bool,
    pub cancellable: bool,
    pub folder_prompt: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub can_up: bool,
    pub show_hidden: bool,
    pub show_ignored: bool,
}
type Listener = Rc<dyn Fn(&ToolbarEvent, &mut Window, &mut App)>;
#[derive(IntoElement)]
pub struct Toolbar {
    state: ToolbarState,
    listener: Listener,
}
impl Toolbar {
    pub fn new(
        state: ToolbarState,
        listener: impl Fn(&ToolbarEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            state,
            listener: Rc::new(listener),
        }
    }
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        icon: IconName,
        disabled: bool,
        event: ToolbarEvent,
    ) -> Button {
        let listener = self.listener.clone();
        let name = match id {
            "compare-project" => "Compare project",
            "compare-local" => "Compare local selection",
            "compare-remote" => "Compare remote selection",
            "compare-marked" => "Compare marked paths",
            _ => label,
        };
        Button::new(id)
            .label(label)
            .accessibility_label(name)
            .tooltip(name)
            .icon(icon)
            .disabled(disabled)
            .on_click(move |_: &ClickEvent, window, cx| listener(&event, window, cx))
    }
}
impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = &self.state;
        let palette = Palette::current(cx);
        let button = |id, label, icon, disabled, event| {
            control(self.button(id, label, icon, disabled, event), cx)
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
        div()
            .id("workspace-toolbar")
            .flex()
            .items_center()
            .flex_wrap()
            .flex_shrink_0()
            .gap(metric(space::CONTROL, cx))
            .px(metric(space::PANEL, cx))
            .py(metric(space::TIGHT, cx))
            .min_h(metric(size::HEADER, cx))
            .text_size(Text::Body.rems())
            .bg(palette.chrome)
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .text_size(Text::Title.rems())
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child("drift"),
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
                        !state.has_location || state.listing,
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
            .child(
                group("toolbar-session")
                    .child(button(
                        "remote",
                        "Remote",
                        IconName::Globe,
                        !state.has_location,
                        ToolbarEvent::Remote,
                    ))
                    .child(button(
                        "local-browser",
                        "Local files",
                        IconName::HardDrive,
                        !state.remote_preview,
                        ToolbarEvent::LocalBrowser,
                    )),
            )
            .child(
                group("toolbar-compare")
                    .child(
                        div()
                            .text_size(Text::Metadata.rems())
                            .text_color(palette.muted)
                            .child("Compare"),
                    )
                    .child(button(
                        "compare-project",
                        "Project",
                        IconName::Replace,
                        !state.can_compare,
                        ToolbarEvent::CompareProject,
                    ))
                    .child(button(
                        "compare-local",
                        "Local selection",
                        IconName::HardDrive,
                        !state.can_compare,
                        ToolbarEvent::CompareLocal,
                    ))
                    .child(button(
                        "compare-remote",
                        "Remote selection",
                        IconName::Globe,
                        !state.can_compare,
                        ToolbarEvent::CompareRemote,
                    ))
                    .child(button(
                        "compare-marked",
                        "Marked paths",
                        IconName::Replace,
                        !state.can_compare,
                        ToolbarEvent::CompareMarked,
                    )),
            )
            .child(
                group("toolbar-navigation")
                    .child(button(
                        "back",
                        "Back",
                        IconName::ArrowLeft,
                        !state.can_back,
                        ToolbarEvent::Browser(BrowserCommand::Back),
                    ))
                    .child(button(
                        "forward",
                        "Forward",
                        IconName::ArrowRight,
                        !state.can_forward,
                        ToolbarEvent::Browser(BrowserCommand::Forward),
                    ))
                    .child(button(
                        "up",
                        "Up",
                        IconName::ArrowUp,
                        !state.can_up,
                        ToolbarEvent::Browser(BrowserCommand::Up),
                    ))
                    .child(button(
                        "refresh",
                        "Refresh",
                        IconName::RefreshCw,
                        false,
                        ToolbarEvent::Browser(BrowserCommand::Refresh),
                    ))
                    .child(button(
                        "find",
                        "Find files",
                        IconName::Search,
                        !state.has_location,
                        ToolbarEvent::Browser(BrowserCommand::Find),
                    )),
            )
            .child(
                group("toolbar-visibility")
                    .child(button(
                        "hidden",
                        if state.show_hidden {
                            "Hide hidden"
                        } else {
                            "Show hidden"
                        },
                        IconName::Eye,
                        false,
                        ToolbarEvent::Browser(BrowserCommand::Hidden),
                    ))
                    .child(button(
                        "ignored",
                        if state.show_ignored {
                            "Hide ignored"
                        } else {
                            "Show ignored"
                        },
                        IconName::EyeOff,
                        false,
                        ToolbarEvent::Browser(BrowserCommand::Ignored),
                    )),
            )
            .child(button(
                "cancel",
                "Cancel",
                IconName::Close,
                !state.cancellable,
                ToolbarEvent::Cancel,
            ))
    }
}
