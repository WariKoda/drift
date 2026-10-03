//! Certificate inspection and decisions; persistence/handshakes belong to drift-app.
use crate::actions::{Cancel, CursorDown, CursorFirst, CursorLast, CursorUp, PageDown, PageUp};
use drift_core::tlstrust::Challenge;
use gpui_kit::base::{Disableable, Selectable};
use gpui_kit::component::{ActiveTheme, button::Button};
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled, Window,
    div, point, px,
};
gpui_kit::actions!(drift_certificates, [NextChoice, PreviousChoice, Confirm]);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NextChoice, Some("DriftCertificate")),
        KeyBinding::new("right", NextChoice, Some("DriftCertificate")),
        KeyBinding::new("l", NextChoice, Some("DriftCertificate")),
        KeyBinding::new("shift-tab", PreviousChoice, Some("DriftCertificate")),
        KeyBinding::new("left", PreviousChoice, Some("DriftCertificate")),
        KeyBinding::new("h", PreviousChoice, Some("DriftCertificate")),
        KeyBinding::new("enter", Confirm, Some("DriftCertificate")),
        KeyBinding::new("up", CursorUp, Some("DriftCertificate")),
        KeyBinding::new("k", CursorUp, Some("DriftCertificate")),
        KeyBinding::new("down", CursorDown, Some("DriftCertificate")),
        KeyBinding::new("j", CursorDown, Some("DriftCertificate")),
        KeyBinding::new("pageup", PageUp, Some("DriftCertificate")),
        KeyBinding::new("pagedown", PageDown, Some("DriftCertificate")),
        KeyBinding::new("home", CursorFirst, Some("DriftCertificate")),
        KeyBinding::new("end", CursorLast, Some("DriftCertificate")),
    ]);
}

#[derive(Clone, Copy)]
pub enum Decision {
    Reject,
    Session,
    Permanent,
}
pub struct CertificatePrompt {
    pub challenge: Challenge,
    pub busy: bool,
    pub error: Option<String>,
    focus: FocusHandle,
    choice: usize,
    scroll: ScrollHandle,
    previous_focus: Option<FocusHandle>,
}
impl EventEmitter<Decision> for CertificatePrompt {}
impl Focusable for CertificatePrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl CertificatePrompt {
    fn decide(&mut self, decision: Decision, cx: &mut Context<Self>) {
        if !self.busy || matches!(decision, Decision::Reject) {
            self.choice = match decision {
                Decision::Reject => 0,
                Decision::Session => 1,
                Decision::Permanent => 2,
            };
            cx.notify();
            cx.emit(decision);
        }
    }
    fn move_choice(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.choice = (self.choice + if next { 1 } else { 2 }) % 3;
        self.focus.focus(window, cx);
        cx.notify();
    }
    fn scroll(&mut self, down: bool, page: bool, cx: &mut Context<Self>) {
        let distance = if page {
            self.scroll.bounds().size.height
        } else {
            px(25.)
        };
        let offset = self.scroll.offset();
        let y = (offset.y + if down { -distance } else { distance })
            .max(-self.scroll.max_offset().y)
            .min(px(0.));
        self.scroll.set_offset(point(offset.x, y));
        cx.notify();
    }
    pub fn previous_focus(&self) -> Option<FocusHandle> {
        self.previous_focus.clone()
    }
    pub fn new(challenge: Challenge, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let previous_focus = window.focused(cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            challenge,
            busy: false,
            error: None,
            focus,
            choice: 0,
            scroll: ScrollHandle::new(),
            previous_focus,
        }
    }
}
impl Render for CertificatePrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let challenge = &self.challenge;
        let details = div()
            .id("certificate-details")
            .flex()
            .flex_col()
            .gap_3()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(format!("Endpoint: {}", challenge.endpoint.address()))
            .child(
                challenge
                    .problems
                    .iter()
                    .map(|p| p.label())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
            .child(format!("Subject: {}", challenge.subject))
            .child(format!("Issuer: {}", challenge.issuer))
            .child(format!("Names: {}", challenge.names.join(", ")))
            .child(format!(
                "Valid from {} until {}",
                challenge.not_before, challenge.not_after
            ))
            .child(
                div()
                    .text_sm()
                    .child(format!("SHA-256: {}", challenge.fingerprint)),
            )
            .children(challenge.previous_fingerprint.as_ref().map(|previous| {
                div()
                    .text_sm()
                    .child(format!("Previous SHA-256: {previous}"))
            }))
            .child(
                "Trust applies to this endpoint, certificate and the listed verification problems.",
            )
            .children(self.error.as_ref().map(|error| div().child(error.clone())));
        div()
            .id("certificate-prompt")
            .key_context("Drift DriftCertificate")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .p_6()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.decide(Decision::Reject, cx)))
            .on_action(cx.listener(|this, _: &NextChoice, w, cx| this.move_choice(true, w, cx)))
            .on_action(
                cx.listener(|this, _: &PreviousChoice, w, cx| this.move_choice(false, w, cx)),
            )
            .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                this.decide(
                    match this.choice {
                        1 => Decision::Session,
                        2 => Decision::Permanent,
                        _ => Decision::Reject,
                    },
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &CursorUp, _, cx| this.scroll(false, false, cx)))
            .on_action(cx.listener(|this, _: &CursorDown, _, cx| this.scroll(true, false, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.scroll(false, true, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.scroll(true, true, cx)))
            .on_action(cx.listener(|this, _: &CursorFirst, _, cx| {
                this.scroll.set_offset(point(px(0.), px(0.)));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CursorLast, _, cx| {
                this.scroll.scroll_to_bottom();
                cx.notify();
            }))
            .child(div().text_lg().child("Verify FTPS certificate"))
            .child(details)
            .child("Tab/Left/Right chooses · Enter confirms · Escape rejects")
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("certificate-reject")
                            .label("Reject")
                            .selected(self.choice == 0)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.decide(Decision::Reject, cx)),
                            ),
                    )
                    .child(
                        Button::new("certificate-session")
                            .label("Trust for this session")
                            .selected(self.choice == 1)
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.decide(Decision::Session, cx)),
                            ),
                    )
                    .child(
                        Button::new("certificate-permanent")
                            .label("Trust permanently")
                            .selected(self.choice == 2)
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.decide(Decision::Permanent, cx)),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests;
