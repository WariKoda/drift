use drift_app::{
    browser::{BrowserService, OperationId},
    hosts::{HostCommand, HostDraft, HostResponse},
};
use drift_core::{
    config::Host,
    pathmap::Mapping,
    store::{HostCatalog, Store},
};
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

struct HostForm {
    expected: Option<Host>,
    fields: Vec<Entity<InputState>>,
    server: String,
    protocol: String,
    auth_kind: String,
    mappings: Vec<(Entity<InputState>, Entity<InputState>)>,
}
impl HostForm {
    fn new(
        host: Host,
        expected: Option<Host>,
        window: &mut Window,
        cx: &mut Context<HostManager>,
    ) -> Self {
        let draft = HostDraft::from(&host);
        let values = [
            draft.name,
            draft.hostname,
            draft.port,
            draft.user,
            draft.root_path,
            draft.keep_alive,
            draft.password,
            draft.key_file,
            draft.passphrase,
        ];
        let fields = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                cx.new(|cx| {
                    let mut state = InputState::new(window, cx)
                        .placeholder(match index {
                            PORT => "Default",
                            KEEP_ALIVE => "60; 0 disables probes",
                            KEY_FILE => "~/.ssh/id_ed25519",
                            _ => FIELDS[index].0,
                        })
                        .masked(matches!(index, PASSWORD | PASSPHRASE));
                    state.set_value(value, window, cx);
                    state
                })
            })
            .collect::<Vec<_>>();
        let mappings = draft
            .mappings
            .into_iter()
            .map(|mapping| {
                let local = cx.new(|cx| {
                    let mut state = InputState::new(window, cx).placeholder("Local path");
                    state.set_value(mapping.local, window, cx);
                    state
                });
                let remote = cx.new(|cx| {
                    let mut state = InputState::new(window, cx).placeholder("Deploy path");
                    state.set_value(mapping.remote, window, cx);
                    state
                });
                (local, remote)
            })
            .collect();
        fields[NAME].focus_handle(cx).focus(window, cx);
        Self {
            expected,
            fields,
            server: draft.server,
            protocol: draft.protocol,
            auth_kind: draft.auth_kind,
            mappings,
        }
    }
    fn draft(&self, cx: &App) -> HostDraft {
        let values: Vec<_> = self
            .fields
            .iter()
            .map(|field| field.read(cx).value().to_string())
            .collect();
        HostDraft {
            name: values[NAME].clone(),
            hostname: values[HOSTNAME].clone(),
            port: values[PORT].clone(),
            user: values[USER].clone(),
            root_path: values[ROOT].clone(),
            keep_alive: values[KEEP_ALIVE].clone(),
            password: values[PASSWORD].clone(),
            key_file: values[KEY_FILE].clone(),
            passphrase: values[PASSPHRASE].clone(),
            server: self.server.clone(),
            protocol: self.protocol.clone(),
            auth_kind: self.auth_kind.clone(),
            mappings: self
                .mappings
                .iter()
                .map(|(local, remote)| Mapping {
                    local: local.read(cx).value().to_string(),
                    remote: remote.read(cx).value().to_string(),
                })
                .collect(),
        }
    }
}

pub struct HostManager {
    store: Store,
    service: BrowserService,
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
    fn field(&self, index: usize) -> AnyElement {
        let form = self.form.as_ref().unwrap();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(FIELDS[index].0)
            .child(
                Input::new(&form.fields[index])
                    .id(FIELDS[index].1)
                    .disabled(self.writing)
                    .when(matches!(index, PASSWORD | PASSPHRASE), |input| {
                        input.mask_toggle()
                    }),
            )
            .into_any_element()
    }
    fn render_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let form = self.form.as_ref().unwrap();
        let linked = !form.server.is_empty();
        let ftp = matches!(form.protocol.as_str(), "ftp" | "ftps");
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(if form.expected.is_some() {
                "Edit host"
            } else {
                "New host"
            })
            .child(self.field(NAME));
        if !self.global {
            view = view.child(
                div().flex().flex_col().gap_2().child("Connection").child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("direct")
                                .label("Own connection")
                                .selected(!linked)
                                .disabled(self.writing)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.form.as_mut().unwrap().server.clear();
                                    cx.notify();
                                })),
                        )
                        .children(
                            self.catalog
                                .as_ref()
                                .into_iter()
                                .flat_map(|catalog| &catalog.servers)
                                .enumerate()
                                .map(|(index, server)| {
                                    let name = server.name.clone();
                                    Button::new(("link", index))
                                        .label(format!("Link {}", name))
                                        .selected(form.server == name)
                                        .disabled(self.writing)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.form.as_mut().unwrap().server = name.clone();
                                            cx.notify();
                                        }))
                                }),
                        ),
                ),
            );
        }
        if linked {
            let endpoint = self
                .catalog
                .as_ref()
                .and_then(|c| c.servers.iter().find(|s| s.name == form.server))
                .map(|s| s.hostname.as_str())
                .unwrap_or("unavailable");
            view = view.child(format!("Server: {} ({endpoint})", form.server));
        } else {
            view = view
                .child(self.field(HOSTNAME))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(div().flex_1().child(self.field(PORT)))
                        .child(div().flex_1().child(self.field(USER))),
                )
                .child(
                    div().flex().flex_wrap().gap_2().child("Protocol").children(
                        [
                            ("", "Default (SFTP)"),
                            ("sftp", "SFTP"),
                            ("ftp", "FTP"),
                            ("ftps", "FTPS"),
                        ]
                        .into_iter()
                        .map(|(value, label)| {
                            Button::new(format!("protocol-{value}"))
                                .label(label)
                                .selected(form.protocol == value)
                                .disabled(self.writing)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.form.as_mut().unwrap().protocol = value.into();
                                    cx.notify();
                                }))
                        }),
                    ),
                )
                .child(self.field(KEEP_ALIVE));
            if !ftp {
                view = view.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child("Authentication")
                        .children(
                            [
                                ("", "Default key file"),
                                ("keyfile", "Key file"),
                                ("password", "Password"),
                                ("agent", "SSH agent"),
                            ]
                            .into_iter()
                            .map(|(value, label)| {
                                Button::new(format!("auth-{value}"))
                                    .label(label)
                                    .selected(form.auth_kind == value)
                                    .disabled(self.writing)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.form.as_mut().unwrap().auth_kind = value.into();
                                        cx.notify();
                                    }))
                            }),
                        ),
                );
            }
            if ftp || form.auth_kind == "password" {
                view = view.child(self.field(PASSWORD));
            } else if form.auth_kind != "agent" {
                view = view
                    .child(self.field(KEY_FILE))
                    .child(self.field(PASSPHRASE));
            }
        }
        view = view
            .child(self.field(ROOT))
            .child("Mappings (relative to local and remote roots)")
            .children(
                form.mappings
                    .iter()
                    .enumerate()
                    .map(|(index, (local, remote))| {
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Input::new(local)
                                    .id(("map-local", index))
                                    .disabled(self.writing),
                            )
                            .child(
                                Input::new(remote)
                                    .id(("map-remote", index))
                                    .disabled(self.writing),
                            )
                            .child(
                                Button::new(("map-remove", index))
                                    .label("Remove")
                                    .disabled(self.writing)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.form.as_mut().unwrap().mappings.remove(index);
                                        cx.notify();
                                    })),
                            )
                    }),
            )
            .child(
                Button::new("map-add")
                    .label("Add mapping")
                    .disabled(self.writing)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let local =
                            cx.new(|cx| InputState::new(window, cx).placeholder("Local path"));
                        let remote =
                            cx.new(|cx| InputState::new(window, cx).placeholder("Deploy path"));
                        local.focus_handle(cx).focus(window, cx);
                        this.form.as_mut().unwrap().mappings.push((local, remote));
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("host-save")
                            .label("Save host")
                            .disabled(self.cancel.is_some())
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(
                        Button::new("host-cancel")
                            .label("Cancel")
                            .disabled(self.writing)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close(&Close, window, cx)),
                            ),
                    ),
            );
        view.into_any_element()
    }
}
impl Render for HostManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            let edit = host.clone(); let duplicate = host.clone(); let delete = host.clone();
                            div().flex().flex_col().gap_1().pb_3().border_b_1().border_color(border)
                                .child(host.name.clone()).child(if host.server.is_empty() { host.hostname.clone() } else { format!("Server link: {}", host.server) })
                                .child(div().flex().gap_1()
                                    .child(Button::new(("host-edit", index)).label("Edit").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.edit(edit.clone(), false, w, cx))))
                                    .child(Button::new(("host-duplicate", index)).label("Duplicate").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, w, cx| this.edit(duplicate.clone(), true, w, cx))))
                                    .child(Button::new(("host-delete", index)).label("Delete").disabled(busy || editing)
                                        .on_click(cx.listener(move |this, _, _, cx| { this.delete = Some(delete.clone()); cx.notify(); }))))
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
            .child(div().p_3().border_t_1().border_color(border).child(self.status.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::{TestAppContextExt, TestWindowExt};
    use gpui_kit::{
        AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, point, size,
    };
    use std::{fs, time::Duration};

    async fn idle(handle: AnyWindowHandle, manager: &Entity<HostManager>, cx: &mut TestAppContext) {
        cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
            manager.read(cx).cancel.is_none()
        })
        .await;
    }

    #[gpui_kit::test]
    async fn real_host_forms_crud_links_conflicts_and_defaults(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        fs::write(
            dir.path().join("config.toml"),
            "[defaults]\nport = 2222\nuser = 'deploy'\n",
        )
        .unwrap();
        let server = Host {
            name: "shared".into(),
            hostname: "server.example".into(),
            root_path: "/srv".into(),
            ..Host::default()
        };
        store.save_host(None, None, server.clone()).unwrap();
        let (handle, manager) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1400.), px(1200.)),
                    })),
                    ..WindowOptions::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        HostManager::new(
                            store.clone(),
                            BrowserService::new().unwrap(),
                            Some("project".into()),
                            window,
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, window, cx| {
            window.click("host-new", cx);
            for (field, value) in [
                ("host-name", "prod"),
                ("hostname", "ftp.example"),
                ("root-path", "/project"),
                ("keep-alive", "0"),
            ] {
                window.click(field, cx);
                window.input(value, cx);
            }
            window.click("protocol-ftps", cx);
            window.click("password", cx);
            window.input("secret", cx);
            window.click("map-add", cx);
            window.click(("map-local", 0usize), cx);
            window.input("src", cx);
            window.click(("map-remote", 0usize), cx);
            window.input("deploy", cx);
            assert!(
                manager.read(cx).form.as_ref().unwrap().fields[PASSWORD]
                    .read(cx)
                    .presentation()
                    .is_masked()
            );
            window.click("host-save", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        let original = store.project("project").unwrap().hosts.remove(0);
        assert_eq!(original.protocol, "ftps");
        assert_eq!(original.port, 0);
        assert_eq!(original.keep_alive_interval, Some(0));
        assert_eq!(original.auth.password, "secret");
        assert_eq!(original.mappings[0].local, "src");
        assert_eq!(store.runtime(Some("project")).unwrap().hosts[0].port, 21);

        cx.update_window(handle, |_, window, cx| {
            window.click(("host-edit", 0usize), cx);
            window.click("hostname", cx);
            window.press(
                if cfg!(target_os = "macos") {
                    "cmd-a"
                } else {
                    "ctrl-a"
                },
                cx,
            );
            window.input("edited.example", cx);
        })
        .unwrap();
        let foreign = Host {
            user: "other process".into(),
            ..original.clone()
        };
        store
            .save_host(Some("project"), Some(&original), foreign.clone())
            .unwrap();
        cx.update_window(handle, |_, window, cx| window.click("host-save", cx))
            .unwrap();
        idle(handle, &manager, cx).await;
        manager.read_with(cx, |manager, cx| {
            assert!(manager.status.contains("changed in another drift process"));
            assert_eq!(
                manager.form.as_ref().unwrap().fields[HOSTNAME]
                    .read(cx)
                    .value()
                    .as_str(),
                "edited.example"
            );
        });
        assert_eq!(
            store.project("project").unwrap().hosts[0].hostname,
            original.hostname
        );
        cx.update_window(handle, |_, window, cx| {
            window.click("host-cancel", cx);
            window.click("hosts-reload", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, window, cx| {
            window.click(("host-edit", 0usize), cx);
            window.click("hostname", cx);
            window.press(
                if cfg!(target_os = "macos") {
                    "cmd-a"
                } else {
                    "ctrl-a"
                },
                cx,
            );
            window.input("edited.example", cx);
            window.click("host-save", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        assert_eq!(
            store.project("project").unwrap().hosts[0].hostname,
            "edited.example"
        );
        cx.update_window(handle, |_, window, cx| {
            window.click(("host-duplicate", 0usize), cx);
            window.click("host-save", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        let hosts = store.project("project").unwrap().hosts;
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[1].name, "prod copy");
        assert_eq!(hosts[1].user, "other process");
        cx.update_window(handle, |_, window, cx| {
            window.click(("host-delete", 1usize), cx);
            window.click("host-confirm-delete", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        assert_eq!(store.project("project").unwrap().hosts.len(), 1);

        cx.update_window(handle, |_, window, cx| window.click("scope-global", cx))
            .unwrap();
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, window, cx| {
            window.click(("host-edit", 0usize), cx);
            let form = manager.read(cx).form.as_ref().unwrap();
            assert!(form.fields[PORT].read(cx).value().is_empty());
            assert!(form.fields[USER].read(cx).value().is_empty());
            window.click("host-save", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        assert!(store.global().unwrap().hosts[0] == server); // resolved defaults never enter the form

        cx.update_window(handle, |_, window, cx| window.click("scope-project", cx))
            .unwrap();
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, window, cx| {
            window.click("host-new", cx);
            window.click("host-name", cx);
            window.input("linked", cx);
            window.click(("link", 0usize), cx);
            window.click("root-path", cx);
            window.input("/linked", cx);
            window.click("host-save", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        let hosts = store.project("project").unwrap().hosts;
        assert_eq!(hosts[1].server, "shared");
        assert!(hosts[1].hostname.is_empty());
        assert!(hosts[1].auth == Default::default());
        assert_eq!(store.runtime(Some("project")).unwrap().hosts[1].port, 2222);
        cx.update_window(handle, |_, window, cx| window.click("scope-global", cx))
            .unwrap();
        idle(handle, &manager, cx).await;
        cx.update_window(handle, |_, window, cx| {
            window.click(("host-delete", 0usize), cx);
            window.click("host-confirm-delete", cx);
        })
        .unwrap();
        idle(handle, &manager, cx).await;
        manager.read_with(cx, |manager, _| {
            assert!(manager.status.contains("linked by project"));
            assert!(manager.delete.is_some());
        });
        assert_eq!(store.global().unwrap().hosts.len(), 1);
    }
}
