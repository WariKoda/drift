use super::*;
use drift_core::store::{LinkCatalog, LinkTarget, LinkedHostSave};
#[cfg(test)]
mod tests;

gpui_kit::actions!(drift_host_offer, [UseLink, KeepOwn, Back]);
pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Back, Some("DriftHostLinkOffer")),
        KeyBinding::new("y", UseLink, Some("DriftHostLinkOffer")),
        KeyBinding::new("n", KeepOwn, Some("DriftHostLinkOffer")),
    ]);
}

pub(super) struct LinkOffer {
    serial: u64,
    expected: Option<Host>,
    desired: Host,
    target: Option<LinkTarget>,
    label: String,
    alternatives: usize,
    phase: Phase,
    pub(super) focus: FocusHandle,
    previous: FocusHandle,
    commit_focus: Option<FocusHandle>,
    reveal: FocusReveal,
}
#[derive(PartialEq, Eq)]
enum Phase {
    Checking,
    Ready,
    Failed,
    Committing,
}

impl HostManager {
    pub(super) fn offer_scene(&self) -> Option<u64> {
        self.offer.as_ref().map(|offer| offer.serial)
    }
    fn offer_owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.active
            && self.offer.as_ref().is_some_and(|offer| {
                offer.focus.is_focused(window)
                    || offer.focus.contains_focused(window, cx)
                    || offer.commit_focus.is_some() && offer.commit_focus == window.focused(cx)
            })
    }
    pub(super) fn start_offer(
        &mut self,
        expected: Option<Host>,
        desired: Host,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = window
            .focused(cx)
            .filter(|focus| self.focus.contains(focus, window))
            .unwrap_or_else(|| self.form.as_ref().unwrap().fields[NAME].focus_handle(cx));
        let focus = cx.focus_handle();
        self.offer_restore = None;
        self.offer = Some(LinkOffer {
            serial: self.operation + 1,
            expected,
            desired: desired.clone(),
            target: None,
            label: String::new(),
            alternatives: 0,
            phase: Phase::Checking,
            focus: focus.clone(),
            previous,
            commit_focus: None,
            reveal: FocusReveal::default(),
        });
        focus.focus(window, cx);
        self.request(
            HostCommand::OfferLink {
                desired: Box::new(desired),
            },
            window,
            cx,
        );
    }
    pub(super) fn offer_loaded(
        &mut self,
        catalog: LinkCatalog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(offer) = &mut self.offer else { return };
        if catalog.targets.is_empty() {
            // No offer exists: keep the original Save and error semantics.
            if !self.offer_is_fresh(window, cx) {
                self.offer_failed();
                return;
            }
            let owns_focus = self.offer_owns_focus(window, cx);
            let offer = self.offer.take().unwrap();
            if owns_focus {
                self.offer_restore = Some((
                    self.operation + 1,
                    offer.previous,
                    Some(self.form.as_ref().unwrap().fields[NAME].entity_id()),
                ));
                self.focus.focus(window, cx);
            }
            self.request(
                HostCommand::Save {
                    expected: offer.expected.map(Box::new),
                    desired: Box::new(offer.desired),
                },
                window,
                cx,
            );
            return;
        }
        offer.alternatives = catalog.targets.len() - 1;
        let target = catalog.targets.into_iter().next().unwrap();
        offer.label = match &target.project {
            None => format!("Global server {:?}", target.host.name),
            Some(slug) => format!(
                "Host {:?} in project {}",
                target.host.name,
                catalog.project_names.get(slug).unwrap_or(slug)
            ),
        };
        offer.target = Some(target);
        offer.phase = Phase::Ready;
    }
    pub(super) fn offer_failed(&mut self) {
        if let Some(offer) = &mut self.offer {
            offer.phase = Phase::Failed;
        }
    }
    fn offer_is_fresh(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(form) = &self.form else { return false };
        if let Err(error) = form_input::validate_inputs(
            form.fields.iter().chain(
                form.mappings
                    .iter()
                    .flat_map(|(local, remote)| [local, remote]),
            ),
            window,
            cx,
        ) {
            self.status = error.into();
            cx.notify();
            return false;
        }
        let valid = self.offer.as_ref().is_some_and(|offer| {
            form.expected == offer.expected
                && form
                    .draft(cx)
                    .build(false)
                    .is_ok_and(|desired| desired == offer.desired)
        });
        if !valid {
            self.back_offer(window, cx);
            self.status = "The host draft changed. Review it before saving.".into();
            cx.notify();
        }
        valid
    }
    pub(super) fn keep_own_offer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.offer_owns_focus(window, cx) {
            return;
        }
        if self.cancel.is_some()
            || self
                .offer
                .as_ref()
                .is_none_or(|offer| offer.phase == Phase::Committing)
            || !self.offer_is_fresh(window, cx)
        {
            return;
        }
        let offer = self.offer.as_mut().unwrap();
        offer.commit_focus = window.focused(cx);
        offer.phase = Phase::Committing;
        let expected = offer.expected.clone().map(Box::new);
        let desired = Box::new(offer.desired.clone());
        self.request(HostCommand::Save { expected, desired }, window, cx);
    }
    pub(super) fn use_link_offer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active
            || !self.offer_owns_focus(window, cx)
            || self.cancel.is_some()
            || self
                .offer
                .as_ref()
                .is_none_or(|offer| offer.phase != Phase::Ready)
            || !self.offer_is_fresh(window, cx)
        {
            return;
        }
        let offer = self.offer.as_mut().unwrap();
        let Some(target) = offer.target.clone() else {
            return;
        };
        if let Err(error) = form_input::validate_values([
            target.host.name.as_str(),
            target.project.as_deref().unwrap_or_default(),
        ]) {
            self.status = error.into();
            cx.notify();
            return;
        }
        offer.commit_focus = window.focused(cx);
        offer.phase = Phase::Committing;
        let expected = offer.expected.clone().map(Box::new);
        let desired = Box::new(offer.desired.clone());
        self.request(
            HostCommand::SaveLink {
                expected,
                desired,
                target: Box::new(target),
            },
            window,
            cx,
        );
    }
    pub(super) fn back_offer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .offer
            .as_ref()
            .is_none_or(|offer| offer.phase == Phase::Committing)
        {
            return;
        }
        let offer = self.offer.take().unwrap();
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.operation += 1;
        self.writing = false;
        self.offer_restore = Some((
            self.operation,
            offer.previous,
            Some(self.form.as_ref().unwrap().fields[NAME].entity_id()),
        ));
        self.focus.focus(window, cx);
        self.status.clear();
        cx.notify();
    }
    pub(super) fn linked_saved(
        &mut self,
        result: LinkedHostSave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = result.destination_error {
            let owns_focus = self.offer_owns_focus(window, cx);
            let previous = self.offer.take().map(|offer| offer.previous);
            if let Some(form) = &mut self.form {
                form.server = result.server.name;
            }
            let form = self.form.as_ref().unwrap();
            let target = previous.unwrap_or_else(|| form.fields[NAME].focus_handle(cx));
            self.offer_restore = owns_focus.then_some((
                self.operation + 1,
                target,
                Some(form.fields[NAME].entity_id()),
            ));
            self.pending_status = Some(format!(
                "Server selection was committed, but this host was not saved: {error}{}",
                result
                    .warning
                    .map_or(String::new(), |warning| format!(" — {warning}"))
            ));
            self.changed = false;
            cx.emit(HostEvent::Changed);
            if owns_focus {
                self.focus.focus(window, cx);
            }
            self.request(HostCommand::Load, window, cx);
        } else {
            self.pending_status = result.warning;
            self.host_saved(window, cx);
        }
    }
    pub(super) fn host_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owns_focus = if self.offer.is_some() {
            self.offer_owns_focus(window, cx)
        } else {
            self.active && self.focus.contains_focused(window, cx)
        };
        if let Some(form) = &self.form {
            self.cursor = Some(form.fields[NAME].read(cx).value().trim().to_owned());
        }
        self.offer = None;
        self.offer_restore = None;
        self.form = None;
        self.delete = None;
        if owns_focus {
            self.focus.focus(window, cx);
            self.offer_restore = Some((self.operation + 1, self.list_focus.clone(), None));
        }
        self.changed = true;
        self.request(HostCommand::Load, window, cx);
    }
    pub(super) fn render_offer(&self, cx: &mut Context<Self>) -> AnyElement {
        let offer = self.offer.as_ref().unwrap();
        let serial = offer.serial;
        let busy = self.cancel.is_some();
        let committing = offer.phase == Phase::Committing;
        div().key_context("DriftHosts DriftHostLinkOffer").track_focus(&offer.focus)
            .size_full().flex().flex_col().min_w_0().min_h_0()
            .capture_any_mouse_down(cx.listener(move |this,_,window,cx| {if !this.active || this.offer_scene()!=Some(serial) {window.prevent_default();cx.stop_propagation();}}))
            .capture_any_mouse_up(cx.listener(move |this,_,window,cx| {if !this.active || this.offer_scene()!=Some(serial) {window.prevent_default();cx.stop_propagation();}}))
            .on_action(cx.listener(|this,_:&Back,w,cx|this.back_offer(w,cx)))
            .on_action(cx.listener(|this,_:&Close,w,cx|this.back_offer(w,cx)))
            .on_action(cx.listener(|this,_:&UseLink,w,cx|this.use_link_offer(w,cx)))
            .on_action(cx.listener(|this,_:&KeepOwn,w,cx|this.keep_own_offer(w,cx)))
            .child(div().id("host-link-offer").p_4().flex_1().min_h_0().min_w_0().overflow_y_scroll().track_scroll(offer.reveal.scroll_handle())
                .map(|view| {#[cfg(test)] {view.test_support()} #[cfg(not(test))] {view}})
                .child(div().flex().flex_col().gap_3().min_w_0()
                    .child("Share this connection?")
                    .child(match offer.phase {Phase::Checking=>"Checking existing servers…".to_owned(),Phase::Failed=>"The matching-server check or save failed. Keep your own connection or return to the form.".into(),_=>offer.label.clone()})
                    .when(offer.target.is_some(),|view|view.child("A matching hostname, port, user and protocol was found. Linking uses that server's authentication and keep-alive settings, not this form's. Your host name, root path and mappings stay in this project."))
                    .when(offer.target.as_ref().is_some_and(|target|target.project.is_some()),|view|view.child("Using this link promotes the other project's host to a global server. Already committed changes are not undone if saving this project's host fails."))
                    .when(offer.alternatives>0,|view|view.child(format!("{} other matching targets. Global servers are preferred, then sorted project hosts.",offer.alternatives)))
                    .child(self.status.clone())
                    .child(div().flex().flex_wrap().gap_2()
                        .child(offer.reveal.wrap("offer-back-reveal",Button::new("host-link-offer-back").label("Back to form").disabled(committing).on_click(cx.listener(|this,_,w,cx|this.back_offer(w,cx)))))
                        .child(offer.reveal.wrap("offer-own-reveal",Button::new("host-link-offer-own").label("Save own connection (n)").disabled(busy||committing).on_click(cx.listener(|this,_,w,cx|this.keep_own_offer(w,cx)))))
                        .when(offer.target.is_some(),|view|view.child(offer.reveal.wrap("offer-link-reveal",Button::new("host-link-offer-use").label("Use server link (y)").disabled(busy||offer.phase!=Phase::Ready).on_click(cx.listener(|this,_,w,cx|this.use_link_offer(w,cx)))))))))
            .into_any_element()
    }
}
