//! Remote browser and host selection. Network tasks belong to drift-app.
use crate::actions::*;
use drift_app::{
    FileList,
    browser::OperationId,
    navigation::History,
    remote::{RemoteDirectory, RemoteService, RemoteSession},
};
use drift_core::{
    config::Host,
    remote::{ConnectionState, RemoteEntry},
};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder;
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollStrategy, StatefulInteractiveElement, Styled,
    Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub enum RemoteEvent {
    SelectionChanged {
        project: u64,
        connection: u64,
    },
    Status {
        project: u64,
        connection: u64,
        message: String,
    },
    Preview {
        session: RemoteSession,
        path: String,
        project: u64,
    },
    ShowLocalPreview,
}
impl EventEmitter<RemoteEvent> for RemotePane {}
#[derive(Clone, Copy)]
enum Navigation {
    Reset,
    Push,
    Seek(usize),
    Keep,
}
pub struct RemotePane {
    service: RemoteService,
    project: u64,
    connection: u64,
    operation: u64,
    hosts: Vec<Host>,
    target: Option<Host>,
    session: Option<RemoteSession>,
    listing: Option<CancellationToken>,
    observer: Option<CancellationToken>,
    path: String,
    entries: Vec<RemoteEntry>,
    files: FileList,
    filter: Entity<InputState>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    history: History,
    navigation: Navigation,
    show_hidden: bool,
    _subscription: Subscription,
}
impl Focusable for RemotePane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Drop for RemotePane {
    fn drop(&mut self) {
        if let Some(cancel) = self.listing.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.observer.take() {
            cancel.cancel();
        }
        if let Some(session) = self.session.take() {
            self.service.close(session);
        }
    }
}
impl RemotePane {
    pub fn new(service: RemoteService, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter remote files…"));
        let subscription = cx.subscribe_in(&filter, window, |this, filter, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.files.filter(&filter.read(cx).value());
                this.selection_changed(cx);
                cx.notify();
            }
        });
        Self {
            service,
            project: 0,
            connection: 0,
            operation: 0,
            hosts: vec![],
            target: None,
            session: None,
            listing: None,
            observer: None,
            path: String::new(),
            entries: vec![],
            files: FileList::new(vec![]),
            filter,
            focus: cx.focus_handle().tab_stop(true),
            scroll: UniformListScrollHandle::new(),
            history: History::default(),
            navigation: Navigation::Reset,
            show_hidden: false,
            _subscription: subscription,
        }
    }
    pub fn set_context(&mut self, project: u64, hosts: Vec<Host>, cx: &mut Context<Self>) {
        if self.project != project {
            self.disconnect(cx);
            self.project = project;
        }
        // This also invalidates an in-flight connection using older credentials.
        if self
            .target
            .as_ref()
            .is_some_and(|target| hosts.iter().find(|h| h.name == target.name) != Some(target))
        {
            self.disconnect(cx);
        }
        self.hosts = hosts;
        cx.notify();
    }
    fn status(&self, message: String, cx: &mut Context<Self>) {
        cx.emit(RemoteEvent::Status {
            project: self.project,
            connection: self.connection,
            message,
        });
    }
    pub fn connection(&self) -> u64 {
        self.connection
    }
    fn selection_changed(&self, cx: &mut Context<Self>) {
        cx.emit(RemoteEvent::SelectionChanged {
            project: self.project,
            connection: self.connection,
        });
    }
    pub fn selected(&self) -> Option<&str> {
        self.files.selected()
    }
    pub fn session_id(&self) -> Option<OperationId> {
        self.session.as_ref().map(|s| s.id)
    }
    pub fn project(&self) -> u64 {
        self.project
    }
    pub fn is_loading(&self) -> bool {
        self.listing.is_some()
    }
    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }
    pub fn comparison_target(&self) -> Option<(Host, RemoteSession)> {
        Some((self.target.clone()?, self.session.clone()?))
    }
    pub fn directory(&self) -> &str {
        &self.path
    }
    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.connection += 1;
        self.operation += 1;
        if let Some(cancel) = self.listing.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.observer.take() {
            cancel.cancel();
        }
        if let Some(session) = self.session.take() {
            self.service.close(session);
        }
        self.target = None;
        self.path.clear();
        self.entries.clear();
        self.files = FileList::new(vec![]);
        self.history = History::default();
        self.selection_changed(cx);
        cx.notify();
    }
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.disconnect(cx);
        self.status("Remote operation cancelled; connection closed".into(), cx);
    }
    fn connect(&mut self, host: Host, window: &mut Window, cx: &mut Context<Self>) {
        self.disconnect(cx);
        self.focus.focus(window, cx);
        self.target = Some(host.clone());
        let connection = self.connection;
        let id = OperationId {
            project: self.project,
            operation: self.operation,
        };
        let operation = self.service.connect(host.clone(), id);
        self.listing = Some(operation.cancel);
        self.navigation = Navigation::Reset;
        self.status(format!("Connecting to {}…", host.name), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.project != id.project
                    || this.connection != connection
                    || this.operation != id.operation
                {
                    return;
                }
                this.listing = None;
                match result {
                    Ok(Ok(directory)) => {
                        this.apply(directory, window, cx);
                        this.observe(connection, window, cx);
                    }
                    Ok(Err(error)) => this.status(error.to_string(), cx),
                    Err(error) => this.status(format!("Remote task failed: {error}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn observe(&mut self, connection: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let project = self.project;
        let operation = self.service.observe(
            session,
            OperationId {
                project,
                operation: connection,
            },
        );
        self.observer = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.connection != connection || this.project != project {
                    return;
                }
                let message = match result {
                    Ok(Ok(ConnectionState::Failed(error))) => error,
                    Ok(Ok(_)) => "Remote connection closed".into(),
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => format!("Connection observer failed: {error}"),
                };
                this.disconnect(cx);
                this.status(message, cx);
            });
        })
        .detach();
    }
    fn apply(&mut self, directory: RemoteDirectory, window: &mut Window, cx: &mut Context<Self>) {
        let path = PathBuf::from(&directory.path);
        match self.navigation {
            Navigation::Reset => self.history.reset(path),
            Navigation::Push => self.history.push(path),
            Navigation::Seek(index) => self.history.commit(index, &path),
            Navigation::Keep => {}
        }
        if !matches!(self.navigation, Navigation::Keep) {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        self.path = directory.path;
        self.session = Some(directory.session);
        self.entries = directory.entries;
        self.set_entries(cx);
        self.status(format!("{} remote entries", self.files.len()), cx);
        cx.notify();
    }
    fn set_entries(&mut self, cx: &mut Context<Self>) {
        self.files = FileList::new(
            self.entries
                .iter()
                .filter(|e| self.show_hidden || !e.name.starts_with('.'))
                .map(|e| e.path.clone())
                .collect(),
        );
        self.files.filter(&self.filter.read(cx).value());
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
    fn navigate(
        &mut self,
        path: String,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if !session.contains(&path) || self.listing.is_some() {
            return;
        }
        self.focus.focus(window, cx);
        self.operation += 1;
        self.selection_changed(cx);
        self.navigation = navigation;
        let id = OperationId {
            project: self.project,
            operation: self.operation,
        };
        let connection = self.connection;
        let operation = self.service.list(session, path, id);
        self.listing = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.project != id.project
                    || this.connection != connection
                    || this.operation != id.operation
                {
                    return;
                }
                this.listing = None;
                match result {
                    Ok(Ok(directory)) => this.apply(directory, window, cx),
                    Ok(Err(error)) => this.status(error.to_string(), cx),
                    Err(error) => this.status(error.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
        self.status("Loading remote directory…".into(), cx);
        cx.notify();
    }
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.listing.is_some() {
            return;
        }
        self.files.select(index);
        let Some(path) = self.files.selected().map(str::to_owned) else {
            return;
        };
        let Some(entry) = self.entries.iter().find(|e| e.path == path) else {
            return;
        };
        self.focus.focus(window, cx);
        if entry.directory {
            self.navigate(path, Navigation::Push, window, cx);
        } else if entry.regular
            && let Some(session) = self.session.clone()
        {
            cx.emit(RemoteEvent::Preview {
                session,
                path,
                project: self.project,
            });
        } else {
            self.status("Remote preview requires a regular file".into(), cx);
        }
        cx.notify();
    }
    fn up(&mut self, _: &GoUp, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = PathBuf::from(&self.path).parent() {
            self.navigate(
                parent.to_string_lossy().into_owned(),
                Navigation::Push,
                window,
                cx,
            );
        }
    }
    fn back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, path)) = self.history.previous() {
            self.navigate(
                path.to_string_lossy().into_owned(),
                Navigation::Seek(index),
                window,
                cx,
            );
        }
    }
    fn forward(&mut self, _: &GoForward, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, path)) = self.history.next() {
            self.navigate(
                path.to_string_lossy().into_owned(),
                Navigation::Seek(index),
                window,
                cx,
            );
        }
    }
    pub fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(self.path.clone(), Navigation::Keep, window, cx);
    }
    pub fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.focus_handle(cx).focus(window, cx);
    }
    fn cursor_up(&mut self, _: &CursorUp, _: &mut Window, cx: &mut Context<Self>) {
        self.files
            .select(self.files.selected_row().unwrap_or(0).saturating_sub(1));
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        self.selection_changed(cx);
        cx.notify();
    }
    fn cursor_down(&mut self, _: &CursorDown, _: &mut Window, cx: &mut Context<Self>) {
        self.files.select(
            self.files
                .selected_row()
                .map_or(0, |i| (i + 1).min(self.files.len().saturating_sub(1))),
        );
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        self.selection_changed(cx);
        cx.notify();
    }
    fn activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        self.choose(self.files.selected_row().unwrap_or(0), window, cx);
    }
    pub fn copy(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.files.selected() {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(path.into()));
        }
    }
}
impl Render for RemotePane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let border = cx.theme().border;
        let background = cx.theme().background;
        let accent = cx.theme().accent;
        div()
            .id("remote-pane")
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::refresh))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .p_2()
                    .child(
                        Button::new("disconnect")
                            .label("Disconnect")
                            .disabled(!self.has_session() && !self.is_loading())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.disconnect(cx);
                                this.status("Disconnected".into(), cx);
                            })),
                    )
                    .child(
                        Button::new("local-preview")
                            .label("Local preview")
                            .on_click(
                                cx.listener(|_, _, _, cx| cx.emit(RemoteEvent::ShowLocalPreview)),
                            ),
                    )
                    .children(self.hosts.iter().cloned().enumerate().map(|(index, host)| {
                        Button::new(("connect-host", index))
                            .label(format!("Connect {}", host.name))
                            .disabled(self.is_loading())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.connect(host.clone(), window, cx)
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_2()
                    .child(
                        Button::new("remote-back")
                            .label("Back")
                            .disabled(self.history.previous().is_none() || self.is_loading())
                            .on_click(cx.listener(|this, _, w, cx| this.back(&GoBack, w, cx))),
                    )
                    .child(
                        Button::new("remote-forward")
                            .label("Forward")
                            .disabled(self.history.next().is_none() || self.is_loading())
                            .on_click(
                                cx.listener(|this, _, w, cx| this.forward(&GoForward, w, cx)),
                            ),
                    )
                    .child(
                        Button::new("remote-up")
                            .label("Up")
                            .disabled(
                                self.session.as_ref().is_none_or(|s| s.root == self.path)
                                    || self.is_loading(),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.up(&GoUp, w, cx))),
                    )
                    .child(
                        Button::new("remote-refresh")
                            .label("Refresh")
                            .disabled(!self.has_session() || self.is_loading())
                            .on_click(cx.listener(|this, _, w, cx| this.refresh(&Refresh, w, cx))),
                    )
                    .child(
                        Button::new("remote-hidden")
                            .label(if self.show_hidden {
                                "Hide hidden"
                            } else {
                                "Show hidden"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_hidden = !this.show_hidden;
                                this.set_entries(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(Input::new(&self.filter).id("remote-filter"))
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(border)
                    .child(if self.path.is_empty() {
                        "Select a host to connect".into()
                    } else {
                        self.path.clone()
                    }),
            )
            .child(
                div()
                    .id("remote-files-pane")
                    .key_context("DriftBrowser")
                    .track_focus(&self.focus)
                    .flex()
                    .flex_col()
                    .on_action(cx.listener(Self::up))
                    .on_action(cx.listener(Self::back))
                    .on_action(cx.listener(Self::forward))
                    .on_action(cx.listener(Self::cursor_up))
                    .on_action(cx.listener(Self::cursor_down))
                    .on_action(cx.listener(Self::activate))
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list("remote-files", self.files.len(), move |range, _, cx| {
                            entity.update(cx, |this, cx| {
                                range
                                    .filter_map(|index| {
                                        let path = this.files.row(index)?.to_owned();
                                        let entry = this.entries.iter().find(|e| e.path == path)?;
                                        let chosen = this.files.selected() == Some(path.as_str());
                                        let row = div()
                                            .id(index)
                                            .h(px(28.))
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .bg(if chosen { accent } else { background })
                                            .child(format!(
                                                "{}{}",
                                                entry.name,
                                                if entry.directory {
                                                    "/"
                                                } else if entry.symlink {
                                                    " →"
                                                } else {
                                                    ""
                                                }
                                            ))
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                this.choose(index, w, cx)
                                            }));
                                        #[cfg(test)]
                                        let row = row.test_support();
                                        Some(row)
                                    })
                                    .collect()
                            })
                        })
                        .track_scroll(&self.scroll)
                        .flex_1(),
                    ),
            )
            .when(self.hosts.is_empty(), |view| {
                view.child(div().p_3().child("Configure a host in Hosts to connect"))
            })
    }
}

#[cfg(test)]
mod tests;
