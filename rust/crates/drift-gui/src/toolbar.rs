//! Stateless shell controls; actions are routed by the session owner.
use crate::browser::BrowserCommand;
use gpui_kit::base::Disableable;
use gpui_kit::component::{ActiveTheme, button::Button};
use gpui_kit::{App, ClickEvent, IntoElement, ParentElement, RenderOnce, Styled, Window, div};
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
        disabled: bool,
        event: ToolbarEvent,
    ) -> Button {
        let listener = self.listener.clone();
        Button::new(id)
            .label(label)
            .disabled(disabled)
            .on_click(move |_: &ClickEvent, window, cx| listener(&event, window, cx))
    }
}
impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = &self.state;
        div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_2()
            .p_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().font_weight(gpui_kit::FontWeight::BOLD).child("drift"))
            .child(self.button("projects", "Projects", false, ToolbarEvent::Projects))
            .child(self.button(
                "remote",
                "Remote",
                !state.has_location,
                ToolbarEvent::Remote,
            ))
            .child(self.button(
                "compare-project",
                "Compare project",
                !state.can_compare,
                ToolbarEvent::CompareProject,
            ))
            .child(self.button(
                "compare-local",
                "Compare local selection",
                !state.can_compare,
                ToolbarEvent::CompareLocal,
            ))
            .child(self.button(
                "compare-remote",
                "Compare remote selection",
                !state.can_compare,
                ToolbarEvent::CompareRemote,
            ))
            .child(self.button(
                "local-browser",
                "Local files",
                !state.remote_preview,
                ToolbarEvent::LocalBrowser,
            ))
            .child(self.button(
                "hosts",
                "Hosts",
                !state.has_location || state.listing,
                ToolbarEvent::Hosts,
            ))
            .child(self.button(
                "open-folder",
                "Open folder",
                state.folder_prompt,
                ToolbarEvent::OpenFolder,
            ))
            .child(self.button(
                "back",
                "Back",
                !state.can_back,
                ToolbarEvent::Browser(BrowserCommand::Back),
            ))
            .child(self.button(
                "forward",
                "Forward",
                !state.can_forward,
                ToolbarEvent::Browser(BrowserCommand::Forward),
            ))
            .child(self.button(
                "up",
                "Up",
                !state.can_up,
                ToolbarEvent::Browser(BrowserCommand::Up),
            ))
            .child(self.button(
                "refresh",
                "Refresh",
                false,
                ToolbarEvent::Browser(BrowserCommand::Refresh),
            ))
            .child(self.button("cancel", "Cancel", !state.cancellable, ToolbarEvent::Cancel))
            .child(self.button(
                "find",
                "Find files",
                !state.has_location,
                ToolbarEvent::Browser(BrowserCommand::Find),
            ))
            .child(self.button(
                "hidden",
                if state.show_hidden {
                    "Hide hidden"
                } else {
                    "Show hidden"
                },
                false,
                ToolbarEvent::Browser(BrowserCommand::Hidden),
            ))
            .child(self.button(
                "ignored",
                if state.show_ignored {
                    "Hide ignored"
                } else {
                    "Show ignored"
                },
                false,
                ToolbarEvent::Browser(BrowserCommand::Ignored),
            ))
    }
}
