//! Separate host-test and trust-reset dialog. All I/O runs through drift-app.
use crate::{
    actions::Cancel,
    certificates::{CertificatePrompt, Decision},
    focus_reveal::FocusReveal,
};
use drift_app::{
    browser::{Operation, OperationId},
    remote::RemoteService,
};
use drift_core::{
    config::Host,
    error::{Error, Result},
    store::Store,
    tlstrust::{Challenge, TrustSnapshot},
};
use gpui_kit::base::Disableable;
use gpui_kit::component::{ActiveTheme, button::Button};
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder,
};
use tokio_util::sync::CancellationToken;

gpui_kit::actions!(drift_trust_reset, [ConfirmReset, ReloadTrust]);
pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("y", ConfirmReset, Some("DriftTrustReset")),
        KeyBinding::new("r", ReloadTrust, Some("DriftTrustReset")),
    ]);
}

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Test,
    Reset,
}
pub(super) struct Closed;
pub(super) struct HostTools {
    store: Store,
    service: RemoteService,
    slug: Option<String>,
    host: Host,
    operation: u64,
    pub(super) cancel: Option<CancellationToken>,
    writing: bool,
    pub(super) status: String,
    reset: Option<TrustSnapshot>,
    certificate: Option<Entity<CertificatePrompt>>,
    restore_focus: Option<(u64, FocusHandle)>,
    temporary_focus: bool,
    subscription: Option<Subscription>,
    focus: FocusHandle,
    reveal: FocusReveal,
    pub(super) previous_focus: Option<FocusHandle>,
}
impl EventEmitter<Closed> for HostTools {}
impl Focusable for HostTools {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Drop for HostTools {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
    }
}
impl HostTools {
    pub(super) fn new(
        store: Store,
        service: RemoteService,
        slug: Option<String>,
        host: Host,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let previous_focus = window.focused(cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut view = Self {
            store,
            service,
            slug,
            host,
            operation: 0,
            cancel: None,
            writing: false,
            status: String::new(),
            reset: None,
            certificate: None,
            restore_focus: None,
            temporary_focus: false,
            subscription: None,
            focus,
            reveal: FocusReveal::default(),
            previous_focus,
        };
        match mode {
            Mode::Test => view.test(None, window, cx),
            Mode::Reset => view.inspect(window, cx),
        }
        view
    }
    fn id(&mut self) -> OperationId {
        self.operation += 1;
        OperationId {
            project: 0,
            operation: self.operation,
        }
    }
    fn await_result<T: Send + 'static>(
        &mut self,
        operation: Operation<T>,
        writing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut Self, Result<T>, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let id = operation.id;
        let temporary_owner = self.temporary_focus && self.focus.is_focused(window);
        let previous_control = window
            .focused(cx)
            .filter(|focus| *focus != self.focus && self.focus.contains(focus, window))
            .or_else(|| {
                if self.temporary_focus && self.focus.is_focused(window) {
                    self.restore_focus.as_ref().map(|(_, focus)| focus.clone())
                } else {
                    None
                }
            });
        self.restore_focus = None;
        self.temporary_focus = previous_control.is_some() || temporary_owner;
        if previous_control.is_some() {
            // Disabled native buttons leave the dispatch tree while work is pending.
            self.focus.focus(window, cx);
        }
        self.cancel = Some(operation.cancel);
        self.writing = writing;
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await.unwrap_or_else(|error| {
                Err(Error::Connection(format!("host operation failed: {error}")))
            });
            let _ = this.update_in(cx, |this, window, cx| {
                if this.operation != id.operation {
                    return;
                }
                this.cancel = None;
                this.writing = false;
                apply(this, result, window, cx);
                if !writing
                    && this.operation == id.operation
                    && this.cancel.is_none()
                    && this.certificate.is_none()
                    && let Some(focus) = &previous_control
                {
                    this.restore_focus = Some((id.operation, focus.clone()));
                } else {
                    this.temporary_focus =
                        !writing && temporary_owner && this.certificate.is_none();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn test(&mut self, required: Option<Challenge>, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        self.reset = None;
        self.status = "Testing connection…".into();
        let id = self.id();
        let operation = self.service.test_host(
            self.store.clone(),
            self.slug.clone(),
            self.host.clone(),
            required,
            id,
        );
        self.await_result(
            operation,
            false,
            window,
            cx,
            |this, result, window, cx| match result {
                Ok(root) => this.status = format!("Connection successful. Root: {root}"),
                Err(Error::Certificate { challenge, .. }) => this.challenge(*challenge, window, cx),
                Err(error) => this.status = error.to_string(),
            },
        );
    }
    fn challenge(&mut self, challenge: Challenge, window: &mut Window, cx: &mut Context<Self>) {
        self.status = "Certificate approval required for connection test".into();
        let prompt = cx.new(|cx| CertificatePrompt::new(challenge, window, cx));
        self.subscription = Some(cx.subscribe_in(
            &prompt,
            window,
            |this, _, decision, window, cx| this.decide(*decision, window, cx),
        ));
        self.certificate = Some(prompt);
    }
    fn decide(&mut self, decision: Decision, window: &mut Window, cx: &mut Context<Self>) {
        if self.certificate.is_none() {
            return;
        }
        if matches!(decision, Decision::Reject) {
            let approval_pending = self.writing;
            if let Some(cancel) = self.cancel.take() {
                cancel.cancel();
            }
            // A trust write may already have committed; discard its result, not its store entry.
            self.operation += 1;
            self.writing = false;
            self.certificate = None;
            self.subscription = None;
            self.status = if approval_pending {
                "Certificate approval cancelled; already saved trust is not undone. Reload/reset trust before retrying."
            } else {
                "FTPS certificate rejected"
            }.into();
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if self.cancel.is_some() {
            return;
        }
        let prompt = self.certificate.as_ref().unwrap();
        let challenge = prompt.read(cx).challenge.clone();
        prompt.update(cx, |prompt, cx| {
            prompt.busy = true;
            prompt.error = None;
            cx.notify();
        });
        let id = self.id();
        let operation = self.service.trust_certificate(
            challenge.clone(),
            matches!(decision, Decision::Permanent),
            id,
        );
        self.await_result(
            operation,
            true,
            window,
            cx,
            move |this, result, window, cx| match result {
                Ok(()) => {
                    this.certificate = None;
                    this.subscription = None;
                    this.focus.focus(window, cx);
                    this.test(Some(challenge), window, cx);
                }
                Err(error) => {
                    if let Some(prompt) = &this.certificate {
                        prompt.update(cx, |prompt, cx| {
                            prompt.busy = false;
                            prompt.error = Some(error.to_string());
                            cx.notify();
                        });
                    }
                }
            },
        );
    }
    fn inspect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        self.status = "Loading certificate trust…".into();
        let id = self.id();
        let operation = self.service.inspect_host_trust(
            self.store.clone(),
            self.slug.clone(),
            self.host.clone(),
            id,
        );
        self.await_result(
            operation,
            false,
            window,
            cx,
            |this, result, _, _| match result {
                Ok(snapshot) => {
                    this.status = "Review certificate trust before resetting".into();
                    this.reset = Some(snapshot);
                }
                Err(error) => {
                    this.reset = None;
                    this.status = error.to_string();
                }
            },
        );
    }
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        let Some(snapshot) = self.reset.clone() else {
            return;
        };
        if snapshot.session.is_none() && snapshot.persistent.is_none() {
            return;
        }
        self.status = "Resetting certificate trust…".into();
        let id = self.id();
        let operation = self.service.reset_host_trust(snapshot, id);
        self.await_result(
            operation,
            true,
            window,
            cx,
            |this, result, _, _| match result {
                Ok(()) => {
                    this.reset = None;
                    this.status =
                        "Certificate trust reset. Future connections require verification again."
                            .into();
                }
                Err(error) => this.status = error.to_string(),
            },
        );
    }
    fn close(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.writing {
            return;
        }
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.operation += 1;
        self.certificate = None;
        self.subscription = None;
        self.restore_focus = None;
        self.temporary_focus = false;
        cx.emit(Closed);
    }
}
impl Render for HostTools {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(prompt) = &self.certificate {
            return div().size_full().child(prompt.clone()).into_any_element();
        }
        let busy = self.cancel.is_some();
        let detail = |id: &'static str, text: String| {
            div()
                .id(id)
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .whitespace_normal()
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
                .child(text)
        };
        let action = |id: &'static str, button: Button| {
            div()
                .min_w_0()
                .max_w_full()
                .child(self.reveal.wrap(id, button.min_w_0().max_w_full()))
        };
        let mut root = div()
            .id("host-tools")
            .relative()
            .key_context(if self.reset.is_some() {
                "Drift DriftTrustReset"
            } else {
                "Drift"
            })
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .p_6()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::close))
            .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, w, cx| {
                if this.reset.is_some()
                    && !this.temporary_focus
                    && this.focus.is_focused(w)
                    && event.keystroke.key == "enter"
                    && event.keystroke.modifiers == Default::default()
                {
                    this.reset(w, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ConfirmReset, w, cx| this.reset(w, cx)))
            .on_action(cx.listener(|this, _: &ReloadTrust, w, cx| this.inspect(w, cx)));
        if let Some((operation, focus)) = self.restore_focus.clone() {
            let entity = cx.weak_entity();
            root = root.child(
                gpui_kit::canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        // Only the completed, enabled control tree can validate the old target.
                        window.defer(cx, move |window, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                if this.operation != operation
                                    || this.restore_focus.as_ref()
                                        != Some(&(operation, focus.clone()))
                                {
                                    return;
                                }
                                this.restore_focus = None;
                                let restore = this.cancel.is_none()
                                    && this.certificate.is_none()
                                    && this.focus.is_focused(window)
                                    && this.focus.contains(&focus, window);
                                // A fallback owner is not an intentional confirmation target.
                                this.temporary_focus = !restore;
                                if restore {
                                    focus.focus(window, cx);
                                }
                            });
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        let mut content = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .w_full()
            .gap_3()
            .child(detail(
                "host-tools-host",
                format!("Host: {}", self.host.name),
            ))
            .child(detail("host-tools-status", self.status.clone()));
        if let Some(snapshot) = &self.reset {
            content = content.child(detail(
                "host-tools-endpoint",
                format!("Endpoint: {}", snapshot.endpoint.address()),
            ));
            for (id, label, record) in [
                (
                    "host-tools-persistent",
                    "Persistent exception",
                    &snapshot.persistent,
                ),
                ("host-tools-session", "Session exception", &snapshot.session),
            ] {
                content = content.child(detail(
                    id,
                    format!(
                        "{label}: {}",
                        record
                            .as_ref()
                            .map_or("None", |entry| entry.fingerprint.as_str())
                    ),
                ));
            }
            content = content
                .child(detail(
                    "host-tools-guidance",
                    "Reset removes both exceptions for this endpoint. Existing connections remain open. Enter/y confirms; r reloads; Escape returns.".into(),
                ))
                .child(
                    div()
                        .id("host-tools-reset-actions")
                        .flex()
                        .flex_wrap()
                        .flex_shrink_0()
                        .min_w_0()
                        .gap_2()
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
                        .child(action(
                            "reveal-host-trust-reset-confirm",
                            Button::new("host-trust-reset-confirm")
                                .label("Reset certificate trust")
                                .disabled(busy || (snapshot.persistent.is_none() && snapshot.session.is_none()))
                                .on_click(cx.listener(|this, _, w, cx| this.reset(w, cx))),
                        ))
                        .child(action(
                            "reveal-host-trust-reload",
                            Button::new("host-trust-reload")
                                .label("Reload trust")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, w, cx| this.inspect(w, cx))),
                        )),
                );
        }
        content = content.child(
            div()
                .flex()
                .flex_wrap()
                .flex_shrink_0()
                .min_w_0()
                .gap_2()
                .child(action(
                    "reveal-host-test-again",
                    Button::new("host-test-again")
                        .label("Test connection")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, w, cx| this.test(None, w, cx))),
                ))
                .child(action(
                    "reveal-host-tools-close",
                    Button::new("host-tools-close")
                        .label(if busy {
                            "Cancel and return"
                        } else {
                            "Return to hosts"
                        })
                        .disabled(self.writing)
                        .on_click(cx.listener(|this, _, w, cx| this.close(&Cancel, w, cx))),
                )),
        );
        root.child(
            div()
                .id("host-tools-scroll")
                .flex_1()
                .min_h_0()
                .min_w_0()
                .overflow_y_scroll()
                .track_scroll(self.reveal.scroll_handle())
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
                .child(content),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod tests;
