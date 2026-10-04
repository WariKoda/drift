use super::*;

pub(super) struct HostForm {
    pub(super) reveal: FocusReveal,
    pub(super) expected: Option<Host>,
    pub(super) fields: Vec<Entity<InputState>>,
    pub(super) server: String,
    pub(super) protocol: String,
    pub(super) auth_kind: String,
    pub(super) mappings: Vec<(Entity<InputState>, Entity<InputState>)>,
}
impl HostForm {
    pub(super) fn new(
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
            reveal: FocusReveal::default(),
            expected,
            fields,
            server: draft.server,
            protocol: draft.protocol,
            auth_kind: draft.auth_kind,
            mappings,
        }
    }
    pub(super) fn draft(&self, cx: &App) -> HostDraft {
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

impl HostManager {
    fn field(&self, index: usize) -> AnyElement {
        let form = self.form.as_ref().unwrap();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(FIELDS[index].0)
            .child(
                form.reveal.wrap(
                    ("host-field", form.fields[index].entity_id()),
                    Input::new(&form.fields[index])
                        .id(FIELDS[index].1)
                        .disabled(self.writing)
                        .when(matches!(index, PASSWORD | PASSPHRASE), |input| {
                            input.mask_toggle()
                        }),
                ),
            )
            .into_any_element()
    }
    pub(super) fn render_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let form = self.form.as_ref().unwrap();
        let linked = !form.server.is_empty();
        let ftp = matches!(form.protocol.as_str(), "ftp" | "ftps");
        let mut view = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
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
                            form.reveal.wrap(
                                "reveal-direct",
                                Button::new("direct")
                                    .label("Own connection")
                                    .selected(!linked)
                                    .disabled(self.writing)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.form.as_mut().unwrap().server.clear();
                                        cx.notify();
                                    })),
                            ),
                        )
                        .child(
                            form.reveal.wrap(
                                "reveal-choose-link",
                                Button::new("host-choose-link")
                                    .label("Choose server / other project")
                                    .disabled(self.writing)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_links(window, cx)
                                    })),
                            ),
                        )
                        .children(
                            self.catalog
                                .as_ref()
                                .into_iter()
                                .flat_map(|catalog| &catalog.servers)
                                .enumerate()
                                .map(|(index, server)| {
                                    let name = server.name.clone();
                                    form.reveal.wrap(
                                        format!("reveal-link-{name}"),
                                        Button::new(("link", index))
                                            .label(format!("Link {}", name))
                                            .selected(form.server == name)
                                            .disabled(self.writing)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.form.as_mut().unwrap().server = name.clone();
                                                cx.notify();
                                            })),
                                    )
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
                        .child(div().flex_1().min_w_0().child(self.field(PORT)))
                        .child(div().flex_1().min_w_0().child(self.field(USER))),
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
                            form.reveal.wrap(
                                format!("reveal-protocol-{value}"),
                                Button::new(format!("protocol-{value}"))
                                    .label(label)
                                    .selected(form.protocol == value)
                                    .disabled(self.writing)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.form.as_mut().unwrap().protocol = value.into();
                                        cx.notify();
                                    })),
                            )
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
                                form.reveal.wrap(
                                    format!("reveal-auth-{value}"),
                                    Button::new(format!("auth-{value}"))
                                        .label(label)
                                        .selected(form.auth_kind == value)
                                        .disabled(self.writing)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.form.as_mut().unwrap().auth_kind = value.into();
                                            cx.notify();
                                        })),
                                )
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
                        let local_id = local.entity_id();
                        div()
                            .id(("mapping-row", local.entity_id()))
                            .flex()
                            .min_w_0()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                div().flex_1().min_w_0().child(
                                    form.reveal.wrap(
                                        ("reveal-map-local", local.entity_id()),
                                        Input::new(local)
                                            .id(("map-local", index))
                                            .disabled(self.writing),
                                    ),
                                ),
                            )
                            .child(
                                div().flex_1().min_w_0().child(
                                    form.reveal.wrap(
                                        ("reveal-map-remote", remote.entity_id()),
                                        Input::new(remote)
                                            .id(("map-remote", index))
                                            .disabled(self.writing),
                                    ),
                                ),
                            )
                            .child(
                                form.reveal.wrap(
                                    ("reveal-map-remove", local.entity_id()),
                                    Button::new(("map-remove", local.entity_id()))
                                        .label("Remove")
                                        .disabled(self.writing)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if this.writing {
                                                return;
                                            }
                                            let Some(form) = this.form.as_mut() else {
                                                return;
                                            };
                                            let Some(index) =
                                                form.mappings.iter().position(|(local, _)| {
                                                    local.entity_id() == local_id
                                                })
                                            else {
                                                return;
                                            };
                                            form.mappings.remove(index);
                                            let focus = form
                                                .mappings
                                                .get(
                                                    index
                                                        .min(form.mappings.len().saturating_sub(1)),
                                                )
                                                .map(|(local, _)| local.focus_handle(cx))
                                                .unwrap_or_else(|| {
                                                    form.fields[ROOT].focus_handle(cx)
                                                });
                                            focus.focus(window, cx);
                                            cx.notify();
                                        })),
                                ),
                            )
                    }),
            )
            .child(
                form.reveal.wrap(
                    "reveal-map-add",
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
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        form.reveal.wrap(
                            "reveal-host-save",
                            Button::new("host-save")
                                .label("Save host (Ctrl/Cmd+S)")
                                .disabled(self.cancel.is_some())
                                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                        ),
                    )
                    .child(
                        form.reveal.wrap(
                            "reveal-host-test-draft",
                            Button::new("host-test-draft")
                                .label("Test connection")
                                .disabled(self.cancel.is_some())
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.test_draft(window, cx)),
                                ),
                        ),
                    )
                    .child(
                        form.reveal.wrap(
                            "reveal-host-cancel",
                            Button::new("host-cancel")
                                .label("Cancel")
                                .disabled(self.writing)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close(&Close, window, cx)
                                })),
                        ),
                    ),
            );
        view.into_any_element()
    }
}
