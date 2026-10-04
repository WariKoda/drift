use super::*;
impl HostManager {
    pub(super) fn open_links(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() || self.global {
            return;
        }
        let Some(slug) = self.project_slug.clone() else {
            return;
        };
        let links = cx
            .new(|cx| LinkPicker::new(self.store.clone(), self.service.clone(), slug, window, cx));
        self.link_subscription =
            Some(
                cx.subscribe_in(&links, window, |this, picker, event, window, cx| {
                    let focus = picker.read(cx).previous_focus.clone();
                    match event {
                        LinkEvent::Close => {
                            this.links = None;
                            this.link_subscription = None;
                            if let Some(focus) = focus {
                                focus.focus(window, cx);
                            } else {
                                this.focus.focus(window, cx);
                            }
                        }
                        LinkEvent::Chosen {
                            server,
                            name,
                            root,
                            warning,
                        } => {
                            this.links = None;
                            this.link_subscription = None;
                            if let Err(error) = form_input::validate_values([
                                server.as_str(),
                                name.as_str(),
                                root.as_str(),
                            ]) {
                                this.pending_status = Some(error.into());
                                this.changed = true;
                                this.request(HostCommand::Load, window, cx);
                                if let Some(focus) = focus {
                                    focus.focus(window, cx);
                                }
                                cx.notify();
                                return;
                            }
                            if let Some(form) = &mut this.form {
                                form.server = server.clone();
                                if let Some(focus) = focus {
                                    focus.focus(window, cx);
                                }
                            } else {
                                let mut candidate = name.clone();
                                let mut suffix = 2;
                                while this
                                    .catalog
                                    .as_ref()
                                    .is_some_and(|c| c.hosts.iter().any(|h| h.name == candidate))
                                {
                                    candidate = format!("{name}-{suffix}");
                                    suffix += 1;
                                }
                                match HostForm::new(
                                    Host {
                                        name: candidate,
                                        server: server.clone(),
                                        root_path: root.clone(),
                                        ..Host::default()
                                    },
                                    None,
                                    window,
                                    cx,
                                ) {
                                    Ok(form) => this.form = Some(form),
                                    Err(error) => {
                                        this.pending_status = Some(error.into());
                                        this.changed = true;
                                        this.request(HostCommand::Load, window, cx);
                                        if let Some(focus) = focus {
                                            focus.focus(window, cx);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                }
                            }
                            this.pending_status = Some(warning.clone().unwrap_or_else(|| {
                                format!("Server {server:?} selected. Save host to store the link.")
                            }));
                            this.changed = true;
                            this.request(HostCommand::Load, window, cx);
                        }
                    }
                    cx.notify();
                }),
            );
        self.links = Some(links);
        cx.notify();
    }
}
