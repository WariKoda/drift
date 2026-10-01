//! Certificate inspection and decisions; persistence/handshakes belong to drift-app.
use crate::actions::Cancel;
use drift_core::tlstrust::Challenge;
use gpui_kit::base::Disableable;
use gpui_kit::component::{ActiveTheme, button::Button};
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, div,
};
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
    previous_focus: Option<FocusHandle>,
}
impl EventEmitter<Decision> for CertificatePrompt {}
impl Focusable for CertificatePrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl CertificatePrompt {
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
            previous_focus,
        }
    }
}
impl Render for CertificatePrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let challenge = &self.challenge;
        div()
            .id("certificate-prompt")
            .key_context("Drift")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .p_6()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(Decision::Reject)))
            .child(div().text_lg().child("Verify FTPS certificate"))
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
            .children(self.error.as_ref().map(|error| div().child(error.clone())))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("certificate-reject")
                            .label("Reject")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Decision::Reject))),
                    )
                    .child(
                        Button::new("certificate-session")
                            .label("Trust for this session")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Decision::Session))),
                    )
                    .child(
                        Button::new("certificate-permanent")
                            .label("Trust permanently")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Decision::Permanent))),
                    ),
            )
    }
}
