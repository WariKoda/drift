use super::*;
use crate::design::{Text, metric, size, space};
use gpui_kit::component::button::Button;
use gpui_kit::{
    AnyElement, EntityId, FocusHandle, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseUpEvent,
    ScrollHandle, StatefulInteractiveElement,
};

pub(super) struct SettingsScreen {
    pub(super) id: u64,
    pub(super) focus: FocusHandle,
    restore: Option<FocusHandle>,
    owner: (u64, Option<EntityId>, bool, bool, PaneView, PaneView),
    scroll: ScrollHandle,
    pub(super) theme: Option<super::preferences::ThemeSelect>,
}

impl Shell {
    fn settings_active(&self, id: u64) -> bool {
        self.help.is_none()
            && self.certificate.is_none()
            && self.settings.as_ref().is_some_and(|s| s.id == id)
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.modal_restore_focus(window, cx);
        self.deactivate_browser_navigation(cx);
        self.comparison
            .update(cx, |pane, cx| pane.deactivate_layout(window, cx));
        if let Some(hosts) = &self.hosts {
            hosts.update(cx, |hosts, _| hosts.deactivate());
        }
        self.settings_serial += 1;
        let focus = cx.focus_handle();
        let theme = self.new_theme_select(self.settings_serial, window, cx);
        self.settings = Some(SettingsScreen {
            id: self.settings_serial,
            focus: focus.clone(),
            restore,
            owner: (
                self.browser.read(cx).id().project,
                self.hosts.as_ref().map(|h| h.entity_id()),
                self.projects.read(cx).visible(),
                self.comparison.read(cx).visible(),
                self.local_view,
                self.remote_view,
            ),
            scroll: ScrollHandle::new(),
            theme,
        });
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.take() else {
            return;
        };
        let owner = (
            self.browser.read(cx).id().project,
            self.hosts.as_ref().map(|h| h.entity_id()),
            self.projects.read(cx).visible(),
            self.comparison.read(cx).visible(),
            self.local_view,
            self.remote_view,
        );
        if settings.owner == owner
            && let Some(restore) = settings.restore
        {
            restore.focus(window, cx);
        } else {
            self.focus_workspace(window, cx);
        }
        cx.notify();
    }

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings.as_ref().expect("settings are visible");
        let id = settings.id;
        let palette = Palette::current(cx);
        div()
            .id("settings-view")
            .test_support()
            .key_context("DriftSettings")
            .track_focus(&settings.focus)
            .flex().flex_col().size_full().min_h_0().min_w_0()
            .bg(palette.canvas).text_color(palette.text)
            .text_size(Text::Body.rems())
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                if !this.settings_active(id) { window.prevent_default(); cx.stop_propagation(); }
            }))
            .capture_any_mouse_up(cx.listener(move |this, _: &MouseUpEvent, window, cx| {
                if !this.settings_active(id) { window.prevent_default(); cx.stop_propagation(); }
            }))
            .capture_key_down(cx.listener(move |this, _: &KeyDownEvent, window, cx| {
                if !this.settings_active(id) { window.prevent_default(); cx.stop_propagation(); }
            }))
            .capture_key_up(cx.listener(move |this, _: &KeyUpEvent, window, cx| {
                if !this.settings_active(id) { window.prevent_default(); cx.stop_propagation(); }
            }))
            .on_action(cx.listener(move |this, _: &Cancel, window, cx| {
                if this.settings_active(id) { this.close_settings(window, cx); }
            }))
            .child(div().flex_shrink_0().px(metric(space::INDENT, cx)).py(metric(space::PANEL, cx))
                .bg(palette.chrome).border_b_1().border_color(palette.border)
                .child(div().text_size(Text::Title.rems()).font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Settings")))
            .child(div().id("settings-body").test_support().flex_1().min_h_0().min_w_0()
                .overflow_y_scroll().track_scroll(&settings.scroll)
                .child(div().w_full().max_w(metric(size::SETTINGS, cx)).min_w_0().flex().flex_col()
                    .p(metric(space::INDENT, cx)).gap(metric(space::INDENT, cx))
                    .child(div().flex().flex_col().gap(metric(space::CONTROL, cx))
                        .child(div().text_size(Text::Title.rems()).child("Appearance"))
                        .child("System follows your operating system. Omarchy follows your Omarchy theme. Dark and Light use a fixed appearance.")
                        .child(if self.preferences.is_some() {
                            self.preference_controls(cx)
                        } else {
                            div().text_color(palette.muted).child("Appearance preferences are unavailable in this session.").into_any_element()
                        }))
                    .child(div().flex().flex_col().gap(metric(space::CONTROL, cx))
                        .child(div().text_size(Text::Title.rems()).child("Window and layout"))
                        .child("Window size, maximized state and pane widths are remembered automatically."))
                    .child(div().text_color(palette.muted).child("Changes are saved automatically."))))
            .child(div().flex().flex_wrap().flex_shrink_0().justify_end()
                .px(metric(space::INDENT, cx)).py(metric(space::CONTROL, cx))
                .border_t_1().border_color(palette.border)
                .child(Button::new("settings-close").label("Close settings")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.settings_active(id) { this.close_settings(window, cx); }
                    }))))
            .into_any_element()
    }
}
