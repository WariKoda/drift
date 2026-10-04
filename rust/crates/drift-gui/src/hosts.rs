mod form;
mod keyboard;
mod linking;
mod links;
mod tools;
use crate::actions::{CursorDown, CursorFirst, CursorLast, CursorUp};
use crate::focus_reveal::FocusReveal;
use crate::form_input;
use drift_app::{
    browser::{BrowserService, OperationId},
    hosts::{HostCommand, HostDraft, HostResponse},
    remote::RemoteService,
};
use drift_core::{
    config::Host,
    pathmap::Mapping,
    store::{HostCatalog, Store},
};
use form::HostForm;
use gpui_kit::base::{Disableable, Selectable};
#[cfg(test)]
use gpui_kit::component::input::Input;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder;
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};
use keyboard::*;
use links::{LinkEvent, LinkPicker};
use tokio_util::sync::CancellationToken;
use tools::{HostTools, Mode};

const NAME: usize = 0;
const HOSTNAME: usize = 1;
const PORT: usize = 2;
const USER: usize = 3;
const ROOT: usize = 4;
const KEEP_ALIVE: usize = 5;
const PASSWORD: usize = 6;
const KEY_FILE: usize = 7;
const PASSPHRASE: usize = 8;
const FIELDS: [(&str, &str); 9] = [
    ("Name", "host-name"),
    ("Hostname", "hostname"),
    ("Port", "port"),
    ("User", "user"),
    ("Root path", "root-path"),
    ("Keep-alive (seconds)", "keep-alive"),
    ("Password", "password"),
    ("Key file", "key-file"),
    ("Passphrase", "passphrase"),
];

gpui_kit::actions!(drift_hosts, [Close, Search]);
pub fn bind_keys(cx: &mut App) {
    keyboard::bind_keys(cx);
    tools::bind_keys(cx);
    links::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("escape", Close, Some("DriftHosts")),
        KeyBinding::new("ctrl-f", Search, Some("DriftHosts")),
        KeyBinding::new("cmd-f", Search, Some("DriftHosts")),
    ]);
}
pub enum HostEvent {
    Close,
    Changed,
}
impl EventEmitter<HostEvent> for HostManager {}

pub struct HostManager {
    store: Store,
    service: BrowserService,
    remote_service: RemoteService,
    tools: Option<Entity<HostTools>>,
    tools_subscription: Option<Subscription>,
    links: Option<Entity<LinkPicker>>,
    link_subscription: Option<Subscription>,
    project_slug: Option<String>,
    global: bool,
    catalog: Option<HostCatalog>,
    form: Option<HostForm>,
    delete: Option<Host>,
    query: Entity<InputState>,
    focus: FocusHandle,
    list_focus: FocusHandle,
    cursor: Option<String>,
    scroll: ScrollHandle,
    operation: u64,
    cancel: Option<CancellationToken>,
    writing: bool,
    changed: bool,
    status: String,
    pending_status: Option<String>,
    _subscription: Subscription,
}
impl Drop for HostManager {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
    }
}
impl HostManager {
    pub fn new(
        store: Store,
        service: BrowserService,
        remote_service: RemoteService,
        project_slug: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Filter hosts…"));
        let subscription = cx.subscribe_in(&query, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.scroll.scroll_to_item(this.cursor_row(cx));
            }
            cx.notify();
        });
        let focus = cx.focus_handle().tab_stop(true);
        query.focus_handle(cx).focus(window, cx);
        let mut manager = Self {
            store,
            service,
            remote_service,
            tools: None,
            tools_subscription: None,
            links: None,
            link_subscription: None,
            global: project_slug.is_none(),
            project_slug,
            catalog: None,
            form: None,
            delete: None,
            query,
            focus,
            list_focus: cx.focus_handle().tab_stop(true),
            cursor: None,
            scroll: ScrollHandle::new(),
            operation: 0,
            cancel: None,
            writing: false,
            changed: false,
            status: String::new(),
            pending_status: None,
            _subscription: subscription,
        };
        manager.request(HostCommand::Load, window, cx);
        manager
    }
    fn request(&mut self, command: HostCommand, window: &mut Window, cx: &mut Context<Self>) {
        if self.writing {
            return;
        }
        self.operation += 1;
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.writing = !matches!(command, HostCommand::Load);
        self.status = if self.writing {
            "Saving changes…"
        } else {
            "Loading hosts…"
        }
        .into();
        let id = OperationId {
            project: 0,
            operation: self.operation,
        };
        let slug = if self.global {
            None
        } else {
            self.project_slug.clone()
        };
        let operation = self
            .service
            .manage_hosts(self.store.clone(), slug, command, id);
        self.cancel = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.operation != id.operation {
                    return;
                }
                this.cancel = None;
                this.writing = false;
                match result {
                    Ok(Ok(HostResponse::Loaded(catalog))) => {
                        this.status = this
                            .pending_status
                            .take()
                            .unwrap_or_else(|| format!("{} hosts", catalog.hosts.len()));
                        this.catalog = Some(*catalog);
                        this.scroll.scroll_to_item(this.cursor_row(cx));
                        if this.changed {
                            this.changed = false;
                            cx.emit(HostEvent::Changed);
                        }
                    }
                    Ok(Ok(HostResponse::Saved | HostResponse::Deleted)) => {
                        if let Some(form) = &this.form {
                            this.cursor =
                                Some(form.fields[NAME].read(cx).value().trim().to_owned());
                        }
                        this.form = None;
                        this.delete = None;
                        this.list_focus.focus(window, cx);
                        this.changed = true;
                        this.request(HostCommand::Load, window, cx);
                    }
                    Ok(Ok(_)) => this.status = "Unexpected host management response".into(),
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Background task failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }
    fn change_scope(&mut self, global: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.list_active() || (!global && self.project_slug.is_none()) {
            return;
        }
        self.global = global;
        self.catalog = None;
        self.cursor = None;
        self.scroll.scroll_to_item(0);
        self.query
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.request(HostCommand::Load, window, cx);
    }
    fn edit(&mut self, host: Host, duplicate: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.list_active() {
            return;
        }
        self.cursor = Some(host.name.clone());
        let expected = (!duplicate).then(|| host.clone());
        let mut host = host;
        if duplicate {
            host.name = format!("{} copy", host.name);
        }
        match HostForm::new(host, expected, window, cx) {
            Ok(form) => {
                self.form = Some(form);
                self.delete = None;
                self.status.clear();
            }
            Err(error) => self.status = error.into(),
        }
        cx.notify();
    }
    fn new_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.list_active() {
            return;
        }
        match HostForm::new(
            Host {
                protocol: "sftp".into(),
                ..Host::default()
            },
            None,
            window,
            cx,
        ) {
            Ok(form) => {
                self.form = Some(form);
                self.delete = None;
                self.status.clear();
            }
            Err(error) => self.status = error.into(),
        }
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &self.form else {
            return;
        };
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
            return;
        }
        match form.draft(cx).build(self.global) {
            Ok(desired) => {
                let expected = form.expected.clone().map(Box::new);
                self.request(
                    HostCommand::Save {
                        expected,
                        desired: Box::new(desired),
                    },
                    window,
                    cx,
                );
            }
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
            }
        }
    }
    fn open_tools(&mut self, host: Host, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        if self.form.is_none() {
            self.cursor = Some(host.name.clone());
        }
        let slug = if self.global {
            None
        } else {
            self.project_slug.clone()
        };
        let tools = cx.new(|cx| {
            HostTools::new(
                self.store.clone(),
                self.remote_service.clone(),
                slug,
                host,
                mode,
                window,
                cx,
            )
        });
        self.tools_subscription = Some(cx.subscribe_in(
            &tools,
            window,
            |this, tools, _: &tools::Closed, window, cx| {
                let focus = tools.read(cx).previous_focus.clone();
                this.tools = None;
                this.tools_subscription = None;
                if let Some(focus) = focus {
                    focus.focus(window, cx);
                } else {
                    this.focus.focus(window, cx);
                }
                cx.notify();
            },
        ));
        self.tools = Some(tools);
        cx.notify();
    }
    fn test_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &self.form else {
            return;
        };
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
            return;
        }
        match form.draft(cx).build(self.global) {
            Ok(host) => self.open_tools(host, Mode::Test, window, cx),
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
            }
        }
    }
    fn close(&mut self, _: &Close, window: &mut Window, cx: &mut Context<Self>) {
        if self.writing {
            return;
        }
        if self.form.take().is_some() || self.delete.take().is_some() {
            self.form = None;
            self.delete = None;
            self.status.clear();
            self.list_focus.focus(window, cx);
            cx.notify();
        } else {
            cx.emit(HostEvent::Close);
        }
    }
    fn search(&mut self, _: &Search, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.is_none() && self.delete.is_none() {
            self.query.focus_handle(cx).focus(window, cx);
        }
    }
}

impl Render for HostManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(tools) = &self.tools {
            return div().size_full().child(tools.clone()).into_any_element();
        }
        if let Some(links) = &self.links {
            return div().size_full().child(links.clone()).into_any_element();
        }
        let selected = self.selected_host(cx).map(|h| h.name);
        let busy = self.cancel.is_some();
        let editing = self.form.is_some() || self.delete.is_some();
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        div().key_context(if self.form.is_some() { "DriftHosts DriftHostForm" } else if self.delete.is_some() { "DriftHosts DriftHostConfirm" } else { "DriftHosts" }).track_focus(&self.focus.clone().tab_stop(self.delete.is_none())).flex().flex_col().size_full()
            .bg(background).text_color(foreground)
            .on_action(cx.listener(Self::close)).on_action(cx.listener(Self::search))
            .on_action(cx.listener(|this, _: &FocusList, w, cx| this.select_row(this.cursor_row(cx), w, cx)))
            .on_action(cx.listener(|this, _: &Save, w, cx| this.save(w, cx)))
            .on_action(cx.listener(|this, _: &Confirm, w, cx| this.confirm_delete(w, cx)))
            .on_action(cx.listener(|this, _: &DefaultConfirm, w, cx| {
                if this.focus.is_focused(w) {
                    this.confirm_delete(w, cx);
                } else {
                    // Native buttons activate on key-up only after an unconsumed key-down.
                    cx.propagate();
                }
            }))
            .child(div().flex().flex_wrap().items_center().gap_2().p_3().border_b_1().border_color(border)
                .child(div().flex_1().child(if self.global { "Global servers".to_string() } else { format!("Project hosts: {}", self.project_slug.as_deref().unwrap_or_default()) }))
                .child(Button::new("scope-project").label("Project hosts").selected(!self.global)
                    .disabled(self.project_slug.is_none() || busy || editing).on_click(cx.listener(|this, _, w, cx| this.change_scope(false, w, cx))))
                .child(Button::new("scope-global").label("Global servers").selected(self.global)
                    .disabled(busy || editing).on_click(cx.listener(|this, _, w, cx| this.change_scope(true, w, cx))))
                .child(Button::new("hosts-close").label("Back to browser").disabled(self.writing)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(HostEvent::Close)))))
            .child(div().flex().flex_1().min_h_0()
                .child(div().flex().flex_col().w(px(350.)).min_h_0().p_3().gap_3().border_r_1().border_color(border)
                    .child(div().key_context("DriftHostFilter").child(form_input::guard(&self.query, |input| input.id("hosts-filter").disabled(editing))))
                    .child(div().flex().gap_2()
                        .child(Button::new("host-new").label("New host").disabled(busy || editing || self.catalog.is_none())
                            .on_click(cx.listener(|this, _, w, cx| this.new_host(w, cx))))
                        .when(!self.global, |view| view.child(Button::new("host-add-link").label("Link host").disabled(busy || editing || self.catalog.is_none())
                            .on_click(cx.listener(|this, _, w, cx| this.open_links(w, cx)))))
                        .child(Button::new("hosts-reload").label("Reload").disabled(self.writing || editing)
                            .on_click(cx.listener(|this, _, w, cx| this.request(HostCommand::Load, w, cx)))))
                    .child(div().id("host-list")
                        .key_context(if editing { "DriftHostListInactive" } else { "DriftHostList" })
                        .track_focus(&self.list_focus.clone().tab_stop(!editing)).track_scroll(&self.scroll)
                        .on_action(cx.listener(|this, _: &CursorUp, w, cx| this.select_row(this.cursor_row(cx).saturating_sub(1), w, cx)))
                        .on_action(cx.listener(|this, _: &CursorDown, w, cx| this.select_row(this.cursor_row(cx) + 1, w, cx)))
                        .on_action(cx.listener(|this, _: &CursorFirst, w, cx| this.select_row(0, w, cx)))
                        .on_action(cx.listener(|this, _: &CursorLast, w, cx| this.select_row(usize::MAX, w, cx)))
                        .on_action(cx.listener(|this, _: &New, w, cx| this.new_host(w, cx)))
                        .on_action(cx.listener(|this, _: &Edit, w, cx| {
                            if !this.list_focus.is_focused(w) {
                                cx.propagate();
                            } else if let Some(host) = this.selected_host(cx) {
                                this.edit(host, false, w, cx);
                            }
                        }))
                        .on_action(cx.listener(|this, _: &Duplicate, w, cx| { if let Some(host) = this.selected_host(cx) { this.edit(host, true, w, cx); } }))
                        .on_action(cx.listener(|this, _: &Remove, w, cx| { if this.cancel.is_none() && let Some(host) = this.selected_host(cx) { this.ask_delete(host, w, cx); } }))
                        .on_action(cx.listener(|this, _: &Test, w, cx| { if this.list_active() && let Some(host) = this.selected_host(cx) { this.open_tools(host, Mode::Test, w, cx); } }))
                        .on_action(cx.listener(|this, _: &ResetTrust, w, cx| this.reset_selected_trust(w, cx)))
                        .on_action(cx.listener(|this, _: &Link, w, cx| { if this.list_active() { this.open_links(w, cx); } }))
                        .on_action(cx.listener(|this, _: &Reload, w, cx| { if this.list_active() { this.request(HostCommand::Load, w, cx); } }))
                        .on_action(cx.listener(|this, _: &Scope, w, cx| {
                            if this.project_slug.is_some() && this.list_active() && this.list_focus.is_focused(w) {
                                this.change_scope(!this.global, w, cx);
                            } else {
                                cx.propagate();
                            }
                        }))
                        .flex().flex_col().gap_3().flex_1().min_h_0().overflow_y_scroll()
                        .children(self.visible_hosts(cx).into_iter().enumerate().map(|(index, host)| {
                            let edit = host.clone(); let duplicate = host.clone(); let delete = host.clone(); let test = host.clone(); let reset = host.clone();
                            let ftps = self.catalog.as_ref().is_some_and(|catalog| catalog.runtime.hosts.iter().any(|resolved| resolved.name == host.name && resolved.protocol == "ftps"));
                            div().id(("host-row", index))
                                .when(selected.as_ref() == Some(&host.name), |view| view.bg(cx.theme().accent))
                                .on_click(cx.listener(move |this, _, w, cx| this.select_row(index, w, cx)))
                                .map(|view| { #[cfg(test)] { view.test_support() } #[cfg(not(test))] { view } })
                                .flex().flex_col().gap_1().pb_3().border_b_1().border_color(border)
                                .child(host.name.clone()).child(if host.server.is_empty() { host.hostname.clone() } else { format!("Server link: {}", host.server) })
                                .child(div().flex().flex_wrap().gap_1()
                                    .child(Button::new(("host-edit", index)).label("Edit").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.edit(edit.clone(), false, w, cx))))
                                    .child(Button::new(("host-duplicate", index)).label("Duplicate").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.edit(duplicate.clone(), true, w, cx))))
                                    .child(Button::new(("host-delete", index)).label("Delete").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.ask_delete(delete.clone(), w, cx))))
                                    .child(Button::new(("host-test", index)).label("Test").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.open_tools(test.clone(), Mode::Test, w, cx))))
                                    .when(ftps, |view| view.child(Button::new(("host-trust-reset", index)).label("Reset certificate trust").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.open_tools(reset.clone(), Mode::Reset, w, cx))))))
                        }))
                        .when(self.catalog.as_ref().is_some_and(|c| c.hosts.is_empty()), |view| view.child("No hosts yet. Add a host to configure a sync target."))))
                .child(div().id("host-details").flex_1().min_w_0().min_h_0().p_4().overflow_y_scroll()
                    .map(|view| { #[cfg(test)] { view.test_support() } #[cfg(not(test))] { view } })
                    .when_some(self.form.as_ref(), |view, form| view.track_scroll(form.reveal.scroll_handle()))
                    .when(self.form.is_some(), |view| view.child(self.render_form(cx)))
                    .when_some(self.delete.as_ref(), |view, host| {
                        view.child(div().flex().flex_col().gap_3()
                            .child(format!("Delete host {:?}?", host.name))
                            .child("Files on the host and in the project remain unchanged.")
                            .child(div().flex().gap_2()
                                .child(Button::new("host-confirm-delete").label("Delete host").disabled(busy)
                                    .on_click(cx.listener(|this, _, w, cx| this.confirm_delete(w, cx))))
                                .child(Button::new("host-cancel-delete").label("Cancel").disabled(self.writing)
                                    .on_click(cx.listener(|this, _, w, cx| this.close(&Close, w, cx))))))
                    })
                    .when(!editing, |view| {
                        let defaults = self.catalog.as_ref().map(|c| format!("Defaults: port {}, user {}. Empty fields keep these defaults.",
                            if c.defaults.port == 0 { "by protocol".into() } else { c.defaults.port.to_string() },
                            if c.defaults.user.is_empty() { "unspecified" } else { &c.defaults.user })).unwrap_or_default();
                        view.child(div().flex().flex_col().gap_3().child("↓ from filter · j/k navigate · n new · e edit · c duplicate · d delete · t test · r reset trust · Tab scope").child(defaults))
                    })))
            .child(div().p_3().border_t_1().border_color(border).child(self.status.clone())).into_any_element()
    }
}

#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod tests;
