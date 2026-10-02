//! Server picker and explicit promotion confirmation, separate from host forms.
use super::*;
use crate::actions::Cancel;
use drift_core::store::{LinkCatalog, LinkTarget};

pub(super) enum LinkEvent {
    Close,
    Chosen {
        server: String,
        name: String,
        root: String,
        warning: Option<String>,
    },
}
pub(super) struct LinkPicker {
    store: Store,
    service: BrowserService,
    slug: String,
    catalog: Option<LinkCatalog>,
    selected: Option<LinkTarget>,
    query: Entity<InputState>,
    focus: FocusHandle,
    pub(super) previous_focus: Option<FocusHandle>,
    operation: u64,
    cancel: Option<CancellationToken>,
    writing: bool,
    status: String,
    _subscription: Subscription,
}
impl EventEmitter<LinkEvent> for LinkPicker {}
impl Drop for LinkPicker {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
    }
}
impl LinkPicker {
    pub(super) fn new(
        store: Store,
        service: BrowserService,
        slug: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let previous_focus = window.focused(cx);
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Filter server, project or hostname…")
        });
        let subscription =
            cx.subscribe_in(&query, window, |_, _, _: &InputEvent, _, cx| cx.notify());
        let focus = cx.focus_handle();
        query.focus_handle(cx).focus(window, cx);
        let mut picker = Self {
            store,
            service,
            slug,
            catalog: None,
            selected: None,
            query,
            focus,
            previous_focus,
            operation: 0,
            cancel: None,
            writing: false,
            status: String::new(),
            _subscription: subscription,
        };
        picker.request(HostCommand::LinkTargets, window, cx);
        picker
    }
    fn request(&mut self, command: HostCommand, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        self.operation += 1;
        self.writing = matches!(command, HostCommand::SelectLink { .. });
        self.status = if self.writing {
            "Selecting server…"
        } else {
            "Loading link targets…"
        }
        .into();
        let operation = self.service.manage_hosts(
            self.store.clone(),
            Some(self.slug.clone()),
            command,
            OperationId {
                project: 0,
                operation: self.operation,
            },
        );
        let id = operation.id;
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
                    Ok(Ok(HostResponse::LinkTargets(catalog))) => {
                        this.status = format!("{} link targets", catalog.targets.len());
                        this.catalog = Some(*catalog);
                        this.selected = None;
                        this.query.focus_handle(cx).focus(window, cx);
                    }
                    Ok(Ok(HostResponse::LinkSelected(promotion))) => {
                        if let Some(selected) = this.selected.take() {
                            cx.emit(LinkEvent::Chosen {
                                server: promotion.server.name,
                                name: selected.host.name,
                                root: selected.host.root_path,
                                warning: promotion.warning,
                            });
                        }
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Link operation failed: {error}"),
                    Ok(Ok(_)) => this.status = "Unexpected link operation response".into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn choose(&mut self, target: LinkTarget, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        let promote = target.project.is_some();
        self.selected = Some(target.clone());
        if promote {
            self.focus.focus(window, cx);
            cx.notify();
        } else {
            self.request(
                HostCommand::SelectLink {
                    expected: Box::new(target),
                },
                window,
                cx,
            );
        }
    }
    fn close(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.writing {
            return;
        }
        if self.selected.take().is_some() {
            self.status.clear();
            self.query.focus_handle(cx).focus(window, cx);
            cx.notify();
        } else {
            if let Some(cancel) = self.cancel.take() {
                cancel.cancel();
            }
            self.operation += 1;
            cx.emit(LinkEvent::Close);
        }
    }
}
impl Render for LinkPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.cancel.is_some();
        let query = self.query.read(cx).value().to_lowercase();
        let mut view = div()
            .id("host-link-picker")
            .key_context("Drift")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .p_5()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::close))
            .child("Link a global server or a host from another project")
            .child(self.status.clone());
        if let Some(selected) = &self.selected {
            let project = selected.project.as_deref().unwrap_or("Global servers");
            view = view.child(if selected.project.is_some() { format!("Promote {:?} from project {project}?", selected.host.name) } else { format!("Use server {:?}?", selected.host.name) })
                .child(format!("{}@{}:{} ({})", selected.host.user, selected.host.hostname, selected.host.port, if selected.host.protocol.is_empty() { "sftp" } else { &selected.host.protocol }))
                .child(if selected.project.is_some() { "The connection and credentials become a global server. The source host becomes a link and keeps its root and mappings. Your new link is saved separately." } else { "Your link uses this server's connection. Save host to store the project link." })
                .child(div().flex().gap_2()
                    .child(Button::new("host-promote-confirm").label(if selected.project.is_some() { "Promote and use server" } else { "Use server" }).disabled(busy).on_click(cx.listener(|this, _, w, cx| {
                        if let Some(target) = this.selected.clone() { this.request(HostCommand::SelectLink { expected: Box::new(target) }, w, cx); }
                    })))
                    .child(Button::new("host-link-reload").label("Reload targets").disabled(busy).on_click(cx.listener(|this, _, w, cx| this.request(HostCommand::LinkTargets, w, cx)))));
        } else {
            view = view
                .child(
                    Input::new(&self.query)
                        .id("host-link-filter")
                        .disabled(busy),
                )
                .child(
                    div()
                        .id("host-link-list")
                        .flex()
                        .flex_col()
                        .gap_3()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(
                            self.catalog
                                .as_ref()
                                .into_iter()
                                .flat_map(|catalog| {
                                    catalog
                                        .targets
                                        .iter()
                                        .enumerate()
                                        .map(move |(index, target)| (index, target, catalog))
                                })
                                .filter(|(_, target, catalog)| {
                                    format!(
                                        "{} {} {} {}",
                                        target.host.name,
                                        target.host.hostname,
                                        target.project.as_deref().unwrap_or("global"),
                                        target
                                            .project
                                            .as_ref()
                                            .and_then(|slug| catalog.project_names.get(slug))
                                            .map_or("", String::as_str)
                                    )
                                    .to_lowercase()
                                    .contains(&query)
                                })
                                .map(|(index, target, catalog)| {
                                    let chosen = target.clone();
                                    let project = target
                                        .project
                                        .as_ref()
                                        .map(|slug| {
                                            catalog.project_names.get(slug).unwrap_or(slug).clone()
                                        })
                                        .unwrap_or_else(|| "Global servers".into());
                                    let users = target
                                        .used_by
                                        .iter()
                                        .map(|slug| {
                                            catalog.project_names.get(slug).unwrap_or(slug).as_str()
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(format!(
                                            "{project}: {} — {}@{}:{}",
                                            target.host.name,
                                            target.host.user,
                                            target.host.hostname,
                                            target.host.port
                                        ))
                                        .child(if users.is_empty() {
                                            String::new()
                                        } else {
                                            format!("Used by: {users}")
                                        })
                                        .child(
                                            Button::new(("host-link-target", index))
                                                .label(if target.project.is_some() {
                                                    "Review promotion"
                                                } else {
                                                    "Use server"
                                                })
                                                .disabled(busy)
                                                .on_click(cx.listener(move |this, _, w, cx| {
                                                    this.choose(chosen.clone(), w, cx)
                                                })),
                                        )
                                }),
                        )
                        .when(
                            self.catalog.as_ref().is_some_and(|c| c.targets.is_empty()),
                            |view| view.child("No global servers or hosts in other projects yet."),
                        ),
                )
                .child(
                    Button::new("host-link-reload")
                        .label("Reload targets")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.request(HostCommand::LinkTargets, w, cx)
                        })),
                );
        }
        view.child(
            Button::new("host-link-close")
                .label(if self.selected.is_some() {
                    "Back to targets"
                } else {
                    "Return to hosts"
                })
                .disabled(self.writing)
                .on_click(cx.listener(|this, _, w, cx| this.close(&Cancel, w, cx))),
        )
    }
}

#[cfg(test)]
mod tests;
