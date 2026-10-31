use super::*;
use crate::preferences::Preferences;
use drift_app::gui_preferences::PreferencesWriter;
use drift_core::gui_preferences::{GuiPreferenceChange, GuiPreferences, ThemePreference};
use gpui_kit::component::{
    IndexPath,
    select::{SearchableVec, Select, SelectEvent, SelectState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseUpEvent, Subscription};

#[cfg(test)]
mod native_dropdown_regressions;

pub(super) struct ThemeSelect {
    state: Entity<SelectState<SearchableVec<&'static str>>>,
    trigger: gpui_kit::FocusHandle,
    _subscriptions: Vec<Subscription>,
}

fn theme_label(mode: ThemePreference) -> &'static str {
    match mode {
        ThemePreference::System => "System",
        ThemePreference::Omarchy => "Omarchy",
        ThemePreference::Dark => "Dark",
        ThemePreference::Light => "Light",
    }
}

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

    pub(super) fn new_theme_select(
        &mut self,
        owner: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ThemeSelect> {
        let preferences = self.preferences.as_ref()?.clone();
        let labels = vec!["System", "Omarchy", "Dark", "Light"];
        let selected = labels
            .iter()
            .position(|label| *label == theme_label(preferences.read(cx).mode));
        let state = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(labels),
                selected.map(IndexPath::new),
                window,
                cx,
            )
        });
        let preference_id = preferences.entity_id();
        let state_id = state.entity_id();
        let trigger = state.focus_handle(cx);
        let confirm = cx.subscribe_in(&state, window, move |this, _, event, window, cx| {
            if !this.theme_select_active(owner, state_id)
                || this.preferences.as_ref().map(|p| p.entity_id()) != Some(preference_id)
            {
                return;
            }
            let SelectEvent::Confirm(Some(label)) = event else {
                return;
            };
            let choice = match *label {
                "System" => ThemePreference::System,
                "Omarchy" => ThemePreference::Omarchy,
                "Dark" => ThemePreference::Dark,
                "Light" => ThemePreference::Light,
                _ => return,
            };
            if let Some(preferences) = &this.preferences {
                preferences.update(cx, |preferences, cx| preferences.select(choice, window, cx));
            }
        });
        let sync_state = state.clone();
        let sync = cx.observe_in(
            &preferences,
            window,
            move |this, preferences, window, cx| {
                if !this.theme_select_active(owner, state_id) {
                    return;
                }
                let label = theme_label(preferences.read(cx).mode);
                if sync_state.read(cx).selected_value() != Some(&label) {
                    sync_state.update(cx, |state, cx| state.set_selected_value(&label, window, cx));
                }
            },
        );
        Some(ThemeSelect {
            state,
            trigger,
            _subscriptions: vec![confirm, sync],
        })
    }

    fn theme_select_active(&self, owner: u64, state: gpui_kit::EntityId) -> bool {
        self.preference_controls_active(owner)
            && self
                .settings
                .as_ref()
                .and_then(|settings| settings.theme.as_ref())
                .is_some_and(|theme| theme.state.entity_id() == state)
    }

    fn guard_theme_select_focus(
        &mut self,
        owner: u64,
        state: gpui_kit::EntityId,
        trigger: gpui_kit::FocusHandle,
        popup: gpui_kit::FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Kit defers confirmation, then unconditionally focuses its trigger.
        // Sample focus before that work and repair only a retired trigger's focus.
        cx.defer_in(window, move |_, window, cx| {
            let before = window.focused(cx);
            cx.defer_in(window, move |this, window, cx| {
                if this.theme_select_active(owner, state) || !trigger.is_focused(window) {
                    return;
                }
                if let Some(before) = before.filter(|focus| *focus != trigger && *focus != popup) {
                    before.focus(window, cx);
                } else if let Some(help) = &this.help {
                    help.focus.focus(window, cx);
                } else if let Some((_, _, certificate)) = &this.certificate {
                    certificate.update(cx, |prompt, cx| prompt.focus(window, cx));
                } else if let Some(settings) = &this.settings {
                    settings.focus.focus(window, cx);
                } else {
                    this.focus_workspace(window, cx);
                }
            });
        });
    }

    fn preference_controls_active(&self, owner: u64) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| settings.id == owner)
            && self.help.is_none()
            && self.certificate.is_none()
            && self.startup.is_none()
            && !self.folder_prompt
    }

    pub(super) fn preference_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(preferences) = &self.preferences else {
            return div().into_any_element();
        };
        let owner = self
            .settings
            .as_ref()
            .expect("theme controls belong to settings")
            .id;
        let Some(theme) = self
            .settings
            .as_ref()
            .and_then(|settings| settings.theme.as_ref())
        else {
            return div().into_any_element();
        };
        let state = theme.state.entity_id();
        let trigger = theme.trigger.clone();
        let popup = theme.state.focus_handle(cx);
        let outside_trigger = trigger.clone();
        let outside_popup = popup.clone();
        let is_open = popup != trigger;
        let confirm_trigger = trigger.clone();
        let confirm_popup = popup.clone();
        let cancel_trigger = trigger.clone();
        let cancel_popup = popup.clone();
        let awaiting_geometry = preferences.read(cx).awaiting_geometry();
        let mut controls = div()
            .id("gui-preferences")
            .test_support()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                if !this.theme_select_active(owner, state) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .capture_any_mouse_up(cx.listener(move |this, _: &MouseUpEvent, window, cx| {
                if !this.theme_select_active(owner, state) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else {
                    this.guard_theme_select_focus(
                        owner,
                        state,
                        trigger.clone(),
                        popup.clone(),
                        window,
                        cx,
                    );
                }
            }))
            // Deferred popup options lie outside the trigger wrapper's hitbox.
            .when(is_open, |control| {
                control.on_mouse_up_out(
                    gpui_kit::MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.guard_theme_select_focus(
                            owner,
                            state,
                            outside_trigger.clone(),
                            outside_popup.clone(),
                            window,
                            cx,
                        );
                    }),
                )
            })
            .capture_key_down(cx.listener(move |this, _: &KeyDownEvent, window, cx| {
                if !this.theme_select_active(owner, state) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(move |this, _: &KeyUpEvent, window, cx| {
                if !this.theme_select_active(owner, state) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    if this.theme_select_active(owner, state) {
                        this.guard_theme_select_focus(
                            owner,
                            state,
                            confirm_trigger.clone(),
                            confirm_popup.clone(),
                            window,
                            cx,
                        );
                        cx.propagate();
                    }
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    if this.theme_select_active(owner, state) {
                        this.guard_theme_select_focus(
                            owner,
                            state,
                            cancel_trigger.clone(),
                            cancel_popup.clone(),
                            window,
                            cx,
                        );
                        cx.propagate();
                    }
                },
            ))
            .child("Theme")
            .child(
                div().w_full().max_w_full().min_w_0().child(
                    Select::new(&theme.state)
                        .id("theme-select")
                        .accessibility_label("Theme")
                        .placeholder("Choose a theme")
                        .disabled(!self.preference_controls_active(owner))
                        .w_full()
                        .min_w_0()
                        .max_w_full(),
                ),
            );
        if awaiting_geometry {
            controls =
                controls.child("Waiting for window state/geometry; saved normal size retained.");
        }
        controls.into_any_element()
    }
}
