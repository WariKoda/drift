use super::*;
use crate::preferences::Preferences;
use drift_app::gui_preferences::PreferencesWriter;
use drift_core::gui_preferences::{GuiPreferenceChange, GuiPreferences, ThemePreference};
use gpui_kit::base::Selectable;
use gpui_kit::component::button::Button;
use gpui_kit::{AnyElement, ClickEvent, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseUpEvent};

impl Shell {
    pub fn with_preferences(
        mut self,
        initial: GuiPreferences,
        writer: PreferencesWriter,
        warning: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let browser_writer = writer.clone();
        self.split
            .configure_persistence(initial.panes.browser, move |value| {
                browser_writer.queue(GuiPreferenceChange::BrowserSplit(value));
            });
        let comparison_writer = writer.clone();
        self.comparison.update(cx, |pane, _| {
            pane.configure_split_persistence(initial.panes.comparison, comparison_writer);
        });
        let preferences = cx.new(|cx| Preferences::new(&initial, writer, warning, window, cx));
        self._subscriptions
            .push(cx.observe(&preferences, |_, _, cx| cx.notify()));
        self.preferences = Some(preferences);
        self
    }

    fn preference_controls_active(&self, cx: &gpui_kit::App) -> bool {
        self.help.is_none()
            && self.certificate.is_none()
            && self.hosts.is_none()
            && self.startup.is_none()
            && !self.folder_prompt
            && !self.projects.read(cx).visible()
    }

    pub(super) fn preference_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(preferences) = &self.preferences else {
            return div().into_any_element();
        };
        let mode = preferences.read(cx).mode;
        let warning = preferences.read(cx).warning.clone();
        let awaiting_geometry = preferences.read(cx).awaiting_geometry();
        let mut controls = div()
            .id("gui-preferences")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if !this.preference_controls_active(cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, _: &MouseUpEvent, _, cx| {
                if !this.preference_controls_active(cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, cx| {
                if !this.preference_controls_active(cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|this, _: &KeyUpEvent, _, cx| {
                if !this.preference_controls_active(cx) {
                    cx.stop_propagation();
                }
            }))
            .child("Theme (Kit):");
        for (choice, id, label) in [
            (ThemePreference::System, "theme-system", "System"),
            (ThemePreference::Dark, "theme-dark", "Dark"),
            (ThemePreference::Light, "theme-light", "Light"),
        ] {
            controls = controls.child(
                Button::new(id)
                    .label(label)
                    .selected(mode == choice)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if !this.preference_controls_active(cx) {
                            return;
                        }
                        if let Some(preferences) = &this.preferences {
                            preferences.update(cx, |preferences, cx| {
                                preferences.select(choice, window, cx)
                            });
                        }
                    })),
            );
        }
        if awaiting_geometry {
            controls =
                controls.child("Waiting for window state/geometry; saved normal size retained.");
        }
        if let Some(warning) = warning {
            controls = controls.child(div().text_color(cx.theme().danger).child(warning));
        }
        controls.into_any_element()
    }
}
