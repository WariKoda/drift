use super::*;
use gpui_kit::component::{
    IndexPath, Sizable,
    select::{SearchableVec, Select, SelectEvent, SelectState},
};

#[derive(Clone, PartialEq)]
struct Scope {
    stamp: navigation::Stamp,
    hosts: Vec<Host>,
    target: Option<String>,
    pending: bool,
}

pub(super) struct HostSelect {
    state: Entity<SelectState<SearchableVec<String>>>,
    scope: Scope,
    trigger: FocusHandle,
    _subscription: Subscription,
}

impl RemotePane {
    fn host_select_scope(&self) -> Scope {
        Scope {
            stamp: self.navigation_stamp(),
            hosts: self.hosts.clone(),
            target: self.target.as_ref().map(|host| host.name.clone()),
            pending: self.pending.is_some(),
        }
    }

    fn host_select_active(&self, scope: &Scope) -> bool {
        self.session_controls_active(scope.stamp)
            && !self.is_loading()
            && self.pending.is_none()
            && *scope == self.host_select_scope()
    }

    pub(crate) fn sync_host_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.host_select_scope();
        if self
            .host_select
            .as_ref()
            .is_some_and(|picker| picker.scope == scope)
        {
            return;
        }
        // A popup belongs to the catalog, connection and workspace frame that
        // opened it. Replacing the state also retires its native deferred popup.
        let names = self
            .hosts
            .iter()
            .map(|host| host.name.clone())
            .collect::<Vec<_>>();
        let selected = names
            .iter()
            .position(|name| Some(name) == scope.target.as_ref());
        let state = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(names),
                selected.map(IndexPath::new),
                window,
                cx,
            )
        });
        let trigger = state.focus_handle(cx);
        let confirmation_scope = scope.clone();
        let subscription =
            cx.subscribe_in(&state, window, move |this, state, event, window, cx| {
                if !this.host_select_active(&confirmation_scope)
                    || this
                        .host_select
                        .as_ref()
                        .map(|picker| picker.state.entity_id())
                        != Some(state.entity_id())
                {
                    return;
                }
                let SelectEvent::Confirm(Some(name)) = event else {
                    return;
                };
                let Some(host) = this.hosts.iter().find(|host| &host.name == name).cloned() else {
                    return;
                };
                if this.target.as_ref() == Some(&host)
                    && this
                        .session
                        .as_ref()
                        .is_some_and(|session| session.state() == ConnectionState::Connected)
                {
                    return;
                }
                this.connect(host, window, cx);
            });
        let was_focused = self
            .host_select
            .as_ref()
            .is_some_and(|picker| picker.state.focus_handle(cx).is_focused(window));
        self.host_select = Some(HostSelect {
            state,
            scope,
            trigger,
            _subscription: subscription,
        });
        if was_focused && self.workspace_active {
            self.focus.focus(window, cx);
        }
    }

    pub(crate) fn host_select_focused(&self, window: &Window, cx: &App) -> bool {
        self.host_select
            .as_ref()
            .is_some_and(|picker| picker.state.focus_handle(cx).is_focused(window))
    }

    fn guard_host_select_focus(
        &mut self,
        scope: Scope,
        trigger: FocusHandle,
        popup: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Native confirmation/cancellation restores trigger focus in deferred
        // work, which may finish after a modal or a catalog update retired it.
        cx.defer_in(window, move |_, window, cx| {
            let before = window.focused(cx);
            cx.defer_in(window, move |this, window, cx| {
                if this.host_select_active(&scope) || !trigger.is_focused(window) {
                    return;
                }
                if let Some(before) = before.filter(|focus| *focus != trigger && *focus != popup) {
                    before.focus(window, cx);
                } else if this.workspace_active {
                    this.focus.focus(window, cx);
                } else {
                    window.blur(cx);
                }
            });
        });
    }

    pub(super) fn host_select_control(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(picker) = &self.host_select else {
            return div().into_any_element();
        };
        let scope = picker.scope.clone();
        let outside_scope = scope.clone();
        let mouse_up_scope = scope.clone();
        let key_down_scope = scope.clone();
        let key_up_scope = scope.clone();
        let confirm_scope = scope.clone();
        let cancel_scope = scope.clone();
        let trigger = picker.trigger.clone();
        let popup = picker.state.focus_handle(cx);
        let outside_trigger = trigger.clone();
        let outside_popup = popup.clone();
        let is_open = popup != trigger;
        let confirm_trigger = trigger.clone();
        let confirm_popup = popup.clone();
        let cancel_trigger = trigger.clone();
        let cancel_popup = popup.clone();
        div()
            .flex_1()
            .min_w_0()
            .w_full()
            .max_w_full()
            .capture_any_mouse_down(cx.listener(
                move |this, _: &gpui_kit::MouseDownEvent, window, cx| {
                    if !this.host_select_active(&scope) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_any_mouse_up(cx.listener(
                move |this, _: &gpui_kit::MouseUpEvent, window, cx| {
                    if !this.host_select_active(&mouse_up_scope) {
                        window.prevent_default();
                        cx.stop_propagation();
                    } else {
                        this.guard_host_select_focus(
                            mouse_up_scope.clone(),
                            trigger.clone(),
                            popup.clone(),
                            window,
                            cx,
                        );
                    }
                },
            ))
            // Popup options are deferred outside the trigger's hitbox, so their
            // pointer confirmations never reach capture_any_mouse_up above.
            .when(is_open, |control| {
                control.on_mouse_up_out(
                    gpui_kit::MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.guard_host_select_focus(
                            outside_scope.clone(),
                            outside_trigger.clone(),
                            outside_popup.clone(),
                            window,
                            cx,
                        );
                    }),
                )
            })
            .capture_key_down(
                cx.listener(move |this, _: &gpui_kit::KeyDownEvent, window, cx| {
                    if !this.host_select_active(&key_down_scope) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_up(
                cx.listener(move |this, _: &gpui_kit::KeyUpEvent, window, cx| {
                    if !this.host_select_active(&key_up_scope) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    if this.host_select_active(&confirm_scope) {
                        this.guard_host_select_focus(
                            confirm_scope.clone(),
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
                    if this.host_select_active(&cancel_scope) {
                        this.guard_host_select_focus(
                            cancel_scope.clone(),
                            cancel_trigger.clone(),
                            cancel_popup.clone(),
                            window,
                            cx,
                        );
                        cx.propagate();
                    }
                },
            ))
            .child(
                Select::new(&picker.state)
                    .id("remote-host-select")
                    .small()
                    .accessibility_label("Remote host")
                    .placeholder(if self.hosts.is_empty() {
                        "No hosts configured"
                    } else {
                        "Connect to host…"
                    })
                    .disabled(!self.host_select_active(&picker.scope) || self.hosts.is_empty())
                    .w_full()
                    .min_w_0()
                    .max_w_full(),
            )
            .into_any_element()
    }
}
