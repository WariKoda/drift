mod form;
mod tools;
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
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, StatefulInteractiveElement,
    Styled, Subscription, Window, div, px,
};
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
    project_slug: Option<String>,
    global: bool,
    catalog: Option<HostCatalog>,
    form: Option<HostForm>,
    delete: Option<Host>,
    query: Entity<InputState>,
    focus: FocusHandle,
    operation: u64,
    cancel: Option<CancellationToken>,
    writing: bool,
    changed: bool,
    status: String,
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
        let subscription =
            cx.subscribe_in(&query, window, |_, _, _: &InputEvent, _, cx| cx.notify());
        let focus = cx.focus_handle().tab_stop(true);
        query.focus_handle(cx).focus(window, cx);
        let mut manager = Self {
            store,
            service,
            remote_service,
            tools: None,
            tools_subscription: None,
            global: project_slug.is_none(),
            project_slug,
            catalog: None,
            form: None,
            delete: None,
            query,
            focus,
            operation: 0,
            cancel: None,
            writing: false,
            changed: false,
            status: String::new(),
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
                        this.status = format!("{} hosts", catalog.hosts.len());
                        this.catalog = Some(*catalog);
                        if this.changed {
                            this.changed = false;
                            cx.emit(HostEvent::Changed);
                        }
                    }
                    Ok(Ok(HostResponse::Saved | HostResponse::Deleted)) => {
                        this.form = None;
                        this.delete = None;
                        this.focus.focus(window, cx);
                        this.changed = true;
                        this.request(HostCommand::Load, window, cx);
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Background task failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn change_scope(&mut self, global: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.writing || self.form.is_some() || self.delete.is_some() {
            return;
        }
        self.global = global;
        self.catalog = None;
        self.query
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.request(HostCommand::Load, window, cx);
    }
    fn edit(&mut self, host: Host, duplicate: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        let expected = (!duplicate).then(|| host.clone());
        let mut host = host;
        if duplicate {
            host.name = format!("{} copy", host.name);
        }
        self.form = Some(HostForm::new(host, expected, window, cx));
        self.delete = None;
        self.status.clear();
        cx.notify();
    }
    fn new_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        self.form = Some(HostForm::new(
            Host {
                protocol: "sftp".into(),
                ..Host::default()
            },
            None,
            window,
            cx,
        ));
        self.delete = None;
        self.status.clear();
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &self.form else {
            return;
        };
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
            self.focus.focus(window, cx);
            cx.notify();
        } else {
            cx.emit(HostEvent::Close);
        }
    }
    fn search(&mut self, _: &Search, window: &mut Window, cx: &mut Context<Self>) {
        self.query.focus_handle(cx).focus(window, cx);
    }
}

impl Render for HostManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(tools) = &self.tools {
            return div().size_full().child(tools.clone()).into_any_element();
        }
        let query = self.query.read(cx).value().to_lowercase();
        let busy = self.cancel.is_some();
        let editing = self.form.is_some() || self.delete.is_some();
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        div().key_context("DriftHosts").track_focus(&self.focus).flex().flex_col().size_full()
            .bg(background).text_color(foreground)
            .on_action(cx.listener(Self::close)).on_action(cx.listener(Self::search))
            .child(div().flex().flex_wrap().items_center().gap_2().p_3().border_b_1().border_color(border)
                .child(div().flex_1().child(if self.global { "Global servers".to_string() } else { format!("Project hosts: {}", self.project_slug.as_deref().unwrap_or_default()) }))
                .child(Button::new("scope-project").label("Project hosts").selected(!self.global)
                    .disabled(self.project_slug.is_none() || self.writing || editing).on_click(cx.listener(|this, _, w, cx| this.change_scope(false, w, cx))))
                .child(Button::new("scope-global").label("Global servers").selected(self.global)
                    .disabled(self.writing || editing).on_click(cx.listener(|this, _, w, cx| this.change_scope(true, w, cx))))
                .child(Button::new("hosts-close").label("Back to browser").disabled(self.writing)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(HostEvent::Close)))))
            .child(div().flex().flex_1().min_h_0()
                .child(div().flex().flex_col().w(px(350.)).min_h_0().p_3().gap_3().border_r_1().border_color(border)
                    .child(Input::new(&self.query).id("hosts-filter"))
                    .child(div().flex().gap_2()
                        .child(Button::new("host-new").label("New host").disabled(busy || editing || self.catalog.is_none())
                            .on_click(cx.listener(|this, _, w, cx| this.new_host(w, cx))))
                        .child(Button::new("hosts-reload").label("Reload").disabled(self.writing || editing)
                            .on_click(cx.listener(|this, _, w, cx| this.request(HostCommand::Load, w, cx)))))
                    .child(div().id("host-list").flex().flex_col().gap_3().flex_1().min_h_0().overflow_y_scroll()
                        .children(self.catalog.as_ref().into_iter().flat_map(|c| &c.hosts).filter(|h| h.name.to_lowercase().contains(&query) || h.hostname.to_lowercase().contains(&query)).enumerate().map(|(index, host)| {
                            let edit = host.clone(); let duplicate = host.clone(); let delete = host.clone(); let test = host.clone(); let reset = host.clone();
                            let ftps = self.catalog.as_ref().is_some_and(|catalog| catalog.runtime.hosts.iter().any(|resolved| resolved.name == host.name && resolved.protocol == "ftps"));
                            div().flex().flex_col().gap_1().pb_3().border_b_1().border_color(border)
                                .child(host.name.clone()).child(if host.server.is_empty() { host.hostname.clone() } else { format!("Server link: {}", host.server) })
                                .child(div().flex().flex_wrap().gap_1()
                                    .child(Button::new(("host-edit", index)).label("Edit").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.edit(edit.clone(), false, w, cx))))
                                    .child(Button::new(("host-duplicate", index)).label("Duplicate").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.edit(duplicate.clone(), true, w, cx))))
                                    .child(Button::new(("host-delete", index)).label("Delete").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, _, cx| { this.delete = Some(delete.clone()); cx.notify(); })))
                                    .child(Button::new(("host-test", index)).label("Test").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.open_tools(test.clone(), Mode::Test, w, cx))))
                                    .when(ftps, |view| view.child(Button::new(("host-trust-reset", index)).label("Reset certificate trust").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.open_tools(reset.clone(), Mode::Reset, w, cx))))))
                        }))
                        .when(self.catalog.as_ref().is_some_and(|c| c.hosts.is_empty()), |view| view.child("No hosts yet. Add a host to configure a sync target."))))
                .child(div().id("host-details").flex_1().min_w_0().p_4().overflow_y_scroll()
                    .when(self.form.is_some(), |view| view.child(self.render_form(cx)))
                    .when_some(self.delete.as_ref(), |view, host| {
                        let expected = host.clone();
                        view.child(div().flex().flex_col().gap_3()
                            .child(format!("Delete host {:?}?", host.name))
                            .child("Files on the host and in the project remain unchanged.")
                            .child(div().flex().gap_2()
                                .child(Button::new("host-confirm-delete").label("Delete host").disabled(busy)
                                    .on_click(cx.listener(move |this, _, w, cx| this.request(HostCommand::Delete { expected: Box::new(expected.clone()) }, w, cx))))
                                .child(Button::new("host-cancel-delete").label("Cancel").disabled(self.writing)
                                    .on_click(cx.listener(|this, _, w, cx| this.close(&Close, w, cx))))))
                    })
                    .when(!editing, |view| {
                        let defaults = self.catalog.as_ref().map(|c| format!("Defaults: port {}, user {}. Empty fields keep these defaults.",
                            if c.defaults.port == 0 { "by protocol".into() } else { c.defaults.port.to_string() },
                            if c.defaults.user.is_empty() { "unspecified" } else { &c.defaults.user })).unwrap_or_default();
                        view.child(div().flex().flex_col().gap_3().child("Select Edit or add a host.").child(defaults))
                    })))
            .child(div().p_3().border_t_1().border_color(border).child(self.status.clone())).into_any_element()
    }
}

#[cfg(test)]
mod tests;
