use super::*;
use crate::certificates::{CertificatePrompt, Decision};
use drift_core::tlstrust::Challenge;
impl Shell {
    pub(super) fn certificate_challenge(
        &mut self,
        project: u64,
        connection: u64,
        challenge: Challenge,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if project != self.browser.read(cx).id().project
            || connection != self.remote.read(cx).connection()
        {
            return;
        }
        self.close_help(window, cx);
        self.close_certificate(window, cx);
        let prompt = cx.new(|cx| CertificatePrompt::new(challenge, window, cx));
        self.certificate_subscription =
            Some(
                cx.subscribe_in(&prompt, window, move |this, _, decision, window, cx| {
                    this.certificate_decision(project, connection, *decision, window, cx)
                }),
            );
        self.certificate = Some((project, connection, prompt));
        cx.notify();
    }
    pub(super) fn close_certificate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(cancel) = self.certificate_cancel.take() {
            cancel.cancel();
        }
        if let Some((project, _, prompt)) = self.certificate.take() {
            let focus = prompt.read(cx).previous_focus();
            if project != self.browser.read(cx).id().project {
                self.browser.update(cx, |pane, cx| pane.focus(window, cx));
            } else if self.comparison.read(cx).visible() && self.hosts.is_none() {
                self.comparison.focus_handle(cx).focus(window, cx);
            } else if let Some(focus) = focus {
                focus.focus(window, cx);
            } else {
                self.browser.update(cx, |pane, cx| pane.focus(window, cx));
            }
        }
        self.certificate_subscription = None;
        cx.notify();
    }
    fn certificate_decision(
        &mut self,
        project: u64,
        connection: u64,
        decision: Decision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((current_project, current_connection, prompt)) = &self.certificate else {
            return;
        };
        if (*current_project, *current_connection) != (project, connection)
            || self.browser.read(cx).id().project != project
            || self.remote.read(cx).connection() != connection
        {
            return;
        }
        if matches!(decision, Decision::Reject) {
            self.close_certificate(window, cx);
            self.remote.update(cx, |pane, cx| pane.disconnect(cx));
            self.status = "FTPS certificate rejected".into();
            return;
        }
        if prompt.read(cx).busy {
            return;
        }
        let challenge = prompt.read(cx).challenge.clone();
        prompt.update(cx, |prompt, cx| {
            prompt.busy = true;
            prompt.error = None;
            cx.notify();
        });
        let operation = self.remote.read(cx).trust_certificate(
            challenge.clone(),
            matches!(decision, Decision::Permanent),
            OperationId {
                project,
                operation: connection,
            },
        );
        self.certificate_cancel = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.certificate.as_ref().map(|(p, c, _)| (*p, *c))
                    != Some((project, connection))
                    || this.browser.read(cx).id().project != project
                    || this.remote.read(cx).connection() != connection
                {
                    return;
                }
                this.certificate_cancel = None;
                match result {
                    Ok(Ok(())) => {
                        let focus = this
                            .certificate
                            .as_ref()
                            .and_then(|(_, _, prompt)| prompt.read(cx).previous_focus());
                        this.close_certificate(window, cx);
                        this.remote.update(cx, |pane, cx| {
                            pane.retry_certificate(project, connection, challenge, window, cx)
                        });
                        // A detached comparison keeps its report; host forms keep
                        // their inputs. Reconnect must not move focus into a pane
                        // hidden behind either view.
                        if this.comparison.read(cx).visible() && this.hosts.is_none() {
                            this.comparison.focus_handle(cx).focus(window, cx);
                        } else if this.hosts.is_some()
                            && let Some(focus) = focus
                        {
                            focus.focus(window, cx);
                        }
                    }
                    failed => {
                        let error = match failed {
                            Ok(Err(error)) => error.to_string(),
                            Err(error) => format!("Certificate trust failed: {error}"),
                            _ => unreachable!(),
                        };
                        if let Some((_, _, prompt)) = &this.certificate {
                            prompt.update(cx, |prompt, cx| {
                                prompt.busy = false;
                                prompt.error = Some(error);
                                cx.notify();
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
