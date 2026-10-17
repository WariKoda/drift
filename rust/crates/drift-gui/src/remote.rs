//! Remote browser and host selection. Network tasks belong to drift-app.
mod menu;
mod tree;
mod visibility;
use crate::actions::*;
use crate::browser_menu::BrowserMenu;
use crate::design::{Palette, Text, control, metric, size, space};
use drift_app::{
    FileList,
    browser::{Location, OperationId},
    navigation::History,
    remote::{RemoteDirectory, RemoteService, RemoteSession, RemoteVisibility},
    tree::FileTree,
};
use drift_core::{
    config::Host,
    error::Error,
    remote::{ConnectionState, RemoteEntry},
    tlstrust::Challenge,
};
use gpui_kit::TestSupportExt;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Disableable, ElementExt};
use gpui_kit::component::Icon;
use gpui_kit::component::{
    button::Button,
    input::{InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Point, Render, ScrollStrategy,
    StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle, Window, div, point,
    px, uniform_list,
};
use menu::Snapshot;
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
    Certificate {
        project: u64,
        connection: u64,
        challenge: Box<Challenge>,
    },
    Compare {
        project: u64,
        connection: u64,
        path: String,
    },
    Toolbar {
        project: u64,
        connection: u64,
        event: crate::toolbar::ToolbarEvent,
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
    pending: Option<Challenge>,
    session: Option<RemoteSession>,
    listing: Option<CancellationToken>,
    observer: Option<CancellationToken>,
    path: String,
    entries: Vec<RemoteEntry>,
    directory_entries: std::collections::BTreeMap<String, RemoteEntry>,
    files: FileList,
    tree: FileTree,
    tree_pending: Option<String>,
    tree_restore: std::collections::VecDeque<String>,
    tree_cursor: Option<String>,
    selection_location: Option<Location>,
    filter: Entity<InputState>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    history: History,
    navigation: Navigation,
    show_hidden: bool,
    show_ignored: bool,
    visibility: Option<RemoteVisibility>,
    visibility_cancel: Option<CancellationToken>,
    visibility_revision: u64,
    menu: Option<(Snapshot, BrowserMenu)>,
    menu_anchor: Point<Pixels>,
    menu_revision: u64,
    comparison_ready: bool,
    _subscription: Subscription,
}
impl Focusable for RemotePane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Drop for RemotePane {
    fn drop(&mut self) {
        if let Some(cancel) = self.visibility_cancel.take() {
            cancel.cancel();
        }
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
        let subscription = cx.subscribe_in(&filter, window, |this, filter, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.dismiss_context_menu(window, cx);
                this.menu_revision += 1;
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
            pending: None,
            session: None,
            listing: None,
            observer: None,
            path: String::new(),
            entries: vec![],
            directory_entries: Default::default(),
            files: FileList::new(vec![]),
            tree: FileTree::default(),
            tree_pending: None,
            tree_restore: Default::default(),
            tree_cursor: None,
            selection_location: None,
            filter,
            focus: cx.focus_handle().tab_stop(true),
            scroll: UniformListScrollHandle::new(),
            history: History::default(),
            navigation: Navigation::Reset,
            show_hidden: false,
            show_ignored: false,
            visibility: None,
            visibility_cancel: None,
            visibility_revision: 0,
            menu: None,
            menu_anchor: point(px(0.), px(0.)),
            menu_revision: 0,
            comparison_ready: false,
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
    pub fn set_selection_location(
        &mut self,
        location: Option<Location>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let unchanged = match (&self.selection_location, &location) {
            (Some(old), Some(new)) => {
                old.root.base() == new.root.base() && old.config.mappings == new.config.mappings
            }
            (None, None) => true,
            _ => false,
        };
        self.selection_location = location;
        if !unchanged {
            self.start_visibility(window, cx);
        }
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
    pub fn marked(&self) -> Vec<String> {
        self.files.marked()
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
        self.listing.is_some() || self.visibility_cancel.is_some()
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
        self.visibility_revision += 1;
        if let Some(cancel) = self.visibility_cancel.take() {
            cancel.cancel();
        }
        self.visibility = None;
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.connection += 1;
        self.operation += 1;
        self.tree_pending = None;
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
        self.pending = None;
        self.path.clear();
        self.entries.clear();
        self.directory_entries.clear();
        self.tree = FileTree::default();
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
        self.connect_required(host, None, window, cx);
    }
    fn connect_required(
        &mut self,
        host: Host,
        required: Option<Challenge>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.disconnect(cx);
        self.focus.focus(window, cx);
        self.target = Some(host.clone());
        let connection = self.connection;
        let id = OperationId {
            project: self.project,
            operation: self.operation,
        };
        let operation = self.service.connect_required(host.clone(), required, id);
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
                    Ok(Err(Error::Certificate { challenge, .. })) => this.challenge(*challenge, cx),
                    Ok(Err(error)) => this.status(error.to_string(), cx),
                    Err(error) => this.status(format!("Remote task failed: {error}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn challenge(&mut self, challenge: Challenge, cx: &mut Context<Self>) {
        self.status(challenge.to_string(), cx);
        self.pending = Some(challenge.clone());
        cx.emit(RemoteEvent::Certificate {
            project: self.project,
            connection: self.connection,
            challenge: Box::new(challenge),
        });
    }
    pub fn retry_certificate(
        &mut self,
        project: u64,
        connection: u64,
        challenge: Challenge,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project != project
            || self.connection != connection
            || self.pending.as_ref() != Some(&challenge)
        {
            return;
        }
        if let Some(host) = self.target.clone() {
            self.connect_required(host, Some(challenge), window, cx);
        }
    }
    pub fn trust_certificate(
        &self,
        challenge: Challenge,
        permanent: bool,
        id: OperationId,
    ) -> drift_app::browser::Operation<()> {
        self.service.trust_certificate(challenge, permanent, id)
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
                if let Ok(Ok(ConnectionState::Certificate(challenge))) = &result {
                    let host = this.target.clone();
                    this.disconnect(cx);
                    this.target = host;
                    this.challenge(*challenge.clone(), cx);
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
        let nodes = directory
            .entries
            .iter()
            .map(|e| (e.path.clone(), e.directory));
        if matches!(self.navigation, Navigation::Keep) {
            self.tree.reload(nodes);
        } else {
            self.tree = FileTree::new(nodes);
        }
        self.entries = directory.entries;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.start_visibility(window, cx);
        cx.notify();
    }
    fn set_entries(&mut self, cx: &mut Context<Self>) {
        self.menu_revision += 1;
        let paths = self
            .tree
            .nodes()
            .iter()
            .filter(|e| {
                self.visibility.as_ref().is_some_and(|visibility| {
                    !visibility.excluded.contains(&e.path)
                        && (self.show_ignored || !visibility.ignored.contains(&e.path))
                })
            })
            .filter(|e| {
                self.show_hidden
                    || !std::path::Path::new(&e.path)
                        .strip_prefix(&self.path)
                        .unwrap_or(std::path::Path::new(&e.path))
                        .components()
                        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
            })
            .map(|e| e.path.clone())
            .collect();
        if let Some(session) = &self.session {
            self.files.replace_remote_entries(&session.root, paths);
        } else {
            self.files.replace_entries(paths);
        }
        if let (Some(location), Some(host), Some(session)) =
            (&self.selection_location, &self.target, &self.session)
        {
            let mut paths = self.files.marked();
            paths.extend(self.entries.iter().map(|entry| entry.path.clone()));
            let directories = self.directory_entries.keys().cloned().collect();
            match drift_app::comparison::remote_markable_paths(
                location,
                host,
                &session.root,
                paths,
                &directories,
            ) {
                Ok(mut allowed) => {
                    if let Some(visibility) = &self.visibility {
                        allowed.retain(|path| !visibility.excluded.contains(path));
                    }
                    self.files.restrict_marks(allowed);
                }
                Err(error) => {
                    self.files.restrict_marks(Default::default());
                    self.status(error.to_string(), cx);
                }
            }
        }
        self.files.filter(&self.filter.read(cx).value());
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
        if !session.contains(&path) || self.is_loading() {
            return;
        }
        self.tree_restore = if matches!(navigation, Navigation::Keep) {
            self.tree.expanded_paths().cloned().collect()
        } else {
            Default::default()
        };
        self.tree_cursor = if matches!(navigation, Navigation::Keep) {
            self.files.selected().map(str::to_owned)
        } else {
            None
        };
        self.focus.focus(window, cx);
        self.operation += 1;
        self.tree_pending = None;
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
        if self.is_loading() {
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
    pub fn preview_selected(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading() {
            return;
        }
        let Some(path) = self.files.selected() else {
            return;
        };
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if session.state() != ConnectionState::Connected || !session.contains(path) {
            return;
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.path == path && entry.regular && !entry.directory)
        {
            cx.emit(RemoteEvent::Preview {
                session: session.clone(),
                path: path.to_owned(),
                project: self.project,
            });
        }
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
    fn cursor_edge(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let row = if last {
            self.files.len().saturating_sub(1)
        } else {
            0
        };
        self.files.select(row);
        self.selection_changed(cx);
        self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        cx.notify();
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
    fn toggle_mark(&mut self, _: &ToggleMark, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.is_loading() {
            cx.propagate();
            return;
        }
        if self
            .files
            .selected()
            .is_some_and(|path| !self.files.can_mark(path))
        {
            self.status("Path is outside the active mappings".into(), cx);
            return;
        }
        self.files.toggle_mark();
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn visual_range(&mut self, _: &VisualRange, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.is_loading() {
            cx.propagate();
            return;
        }
        self.files.visual_range();
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn mark_all(&mut self, _: &MarkAll, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.is_loading() {
            cx.propagate();
            return;
        }
        self.files.mark_siblings();
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn invert_marks(&mut self, _: &InvertMarks, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.is_loading() {
            cx.propagate();
            return;
        }
        self.files.invert_visible();
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn clear_marks(&mut self, _: &ClearMarks, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        if self.files.range_active() {
            self.files.clear_marks();
        } else if !self.filter.read(cx).value().is_empty() {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
            // Programmatic input changes do not emit InputEvent::Change.
            self.files.filter("");
            self.selection_changed(cx);
        } else {
            self.files.clear_marks();
        }
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn range_up(&mut self, _: &RangeUp, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.is_loading() {
            cx.propagate();
            return;
        }
        let index = self.files.selected_row().unwrap_or(0).saturating_sub(1);
        self.files.select_range(index);
        self.selection_changed(cx);
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn range_down(&mut self, _: &RangeDown, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.is_loading() {
            cx.propagate();
            return;
        }
        let index = self
            .files
            .selected_row()
            .map_or(0, |i| (i + 1).min(self.files.len().saturating_sub(1)));
        self.files.select_range(index);
        self.selection_changed(cx);
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn open_directory(&mut self, _: &OpenDirectory, window: &mut Window, cx: &mut Context<Self>) {
        self.choose(self.files.selected_row().unwrap_or(0), window, cx);
    }
    fn activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.files.selected_row().unwrap_or(0);
        self.files.select(index);
        let Some(path) = self.files.selected().map(str::to_owned) else {
            return;
        };
        if self.tree.node(&path).is_some_and(|n| n.directory) {
            if self.tree.node(&path).is_some_and(|n| n.expanded) {
                if let Some(child) = self.files.row(index + 1).map(str::to_owned)
                    && self.tree.parent(&child) == Some(path.as_str())
                {
                    self.files.select(index + 1);
                }
                self.selection_changed(cx);
                self.scroll.scroll_to_item(
                    self.files.selected_row().unwrap_or(0),
                    ScrollStrategy::Nearest,
                );
                cx.notify();
            } else {
                self.expand(path, window, cx);
            }
        } else {
            self.choose(index, window, cx);
        }
    }
    pub fn copy(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.files.selected() {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(path.into()));
        }
    }
}
impl Render for RemotePane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .menu
            .as_ref()
            .is_some_and(|(snapshot, _)| !self.menu_valid(snapshot, cx))
        {
            self.dismiss_context_menu(window, cx);
        }
        let entity = cx.entity();
        let anchor_entity = entity.clone();
        let context_stamp = (
            self.project,
            self.connection,
            self.operation,
            self.menu_revision,
        );
        let palette = Palette::current(cx);
        let border = palette.border;
        let background = palette.canvas;
        let accent = palette.selected;
        let empty_anchor_offset = metric(space::CONTROL, cx);
        div()
            .id("remote-pane")
            .text_size(Text::Body.rems())
            .text_color(palette.text)
            .bg(palette.canvas)
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
                    .gap(metric(space::TIGHT, cx))
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .min_h(metric(size::HEADER, cx))
                    .bg(palette.chrome)
                    .child(
                        control(Button::new("disconnect"), cx)
                            .icon(IconName::Close)
                            .label("Disconnect")
                            .disabled(!self.has_session() && !self.is_loading())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.disconnect(cx);
                                this.status("Disconnected".into(), cx);
                            })),
                    )
                    .child(
                        control(Button::new("local-preview"), cx)
                            .icon(IconName::FileText)
                            .label("Local preview")
                            .on_click(
                                cx.listener(|_, _, _, cx| cx.emit(RemoteEvent::ShowLocalPreview)),
                            ),
                    )
                    .children(self.hosts.iter().cloned().enumerate().map(|(index, host)| {
                        control(Button::new(("connect-host", index)), cx)
                            .min_w_0()
                            .max_w_full()
                            .overflow_hidden()
                            .icon(IconName::Network)
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
                    .flex_wrap()
                    .gap(metric(space::TIGHT, cx))
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .bg(palette.chrome)
                    .child(
                        control(Button::new("remote-back"), cx)
                            .icon(IconName::ArrowLeft)
                            .label("Back")
                            .disabled(self.history.previous().is_none() || self.is_loading())
                            .on_click(cx.listener(|this, _, w, cx| this.back(&GoBack, w, cx))),
                    )
                    .child(
                        control(Button::new("remote-forward"), cx)
                            .icon(IconName::ArrowRight)
                            .label("Forward")
                            .disabled(self.history.next().is_none() || self.is_loading())
                            .on_click(
                                cx.listener(|this, _, w, cx| this.forward(&GoForward, w, cx)),
                            ),
                    )
                    .child(
                        control(Button::new("remote-up"), cx)
                            .icon(IconName::ArrowUp)
                            .label("Up")
                            .disabled(
                                self.session.as_ref().is_none_or(|s| s.root == self.path)
                                    || self.is_loading(),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.up(&GoUp, w, cx))),
                    )
                    .child(
                        control(Button::new("remote-refresh"), cx)
                            .icon(IconName::RefreshCw)
                            .label("Refresh")
                            .disabled(!self.has_session() || self.is_loading())
                            .on_click(cx.listener(|this, _, w, cx| this.refresh(&Refresh, w, cx))),
                    )
                    .child(
                        control(Button::new("remote-ignored"), cx)
                            .icon(IconName::EyeOff)
                            .label(if self.show_ignored { "Hide ignored" } else { "Show ignored" })
                            .disabled(self.is_loading() || !self.has_session() || self.selection_location.is_none() || self.visibility.is_none())
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_ignored(window, cx))),
                    )
                    .child(
                        control(Button::new("remote-hidden"), cx)
                            .icon(IconName::Eye)
                            .disabled(self.is_loading())
                            .label(if self.show_hidden {
                                "Hide hidden"
                            } else {
                                "Show hidden"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.is_loading() {
                                    return;
                                }
                                this.show_hidden = !this.show_hidden;
                                this.set_entries(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .key_context("DriftRemoteFilter")
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .on_action(cx.listener(|this, _: &FocusResults, w, cx| {
                        this.focus.focus(w, cx);
                    }))
                    .on_action(cx.listener(|this, _: &Cancel, w, cx| {
                        this.focus.focus(w, cx);
                    }))
                    .on_action(cx.listener(|this, _: &gpui_kit::component::input::Escape, w, cx| {
                        this.focus.focus(w, cx);
                    }))
                    .child(crate::form_input::guard(&self.filter, |input| input.id("remote-filter"))),
            )
            .child(
                div()
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .text_size(Text::Metadata.rems())
                    .bg(palette.chrome)
                    .border_b_1()
                    .border_color(border)
                    .child(if self.path.is_empty() {
                        "Select a host to connect".into()
                    } else {
                        format!(
                            "{} · {} marked{}",
                            self.path,
                            self.files.marked().len(),
                            if self.files.range_active() {
                                " · range: press v to finish"
                            } else {
                                ""
                            }
                        )
                    }),
            )
            .child(
                div()
                    .id("remote-files-pane")
                    .test_support()
                    .key_context("DriftBrowser")
                    .track_focus(&self.focus)
                    .border_l(metric(2., cx))
                    .border_color(palette.canvas)
                    .focus(move |style| style.border_color(palette.focus))
                    .on_prepaint(move |bounds, _, cx| {
                        anchor_entity.update(cx, |this, _| {
                            if this.files.selected_row().is_none() {
                                this.menu_anchor = bounds.origin + point(empty_anchor_offset, empty_anchor_offset);
                            }
                        });
                    })
                    .on_action(cx.listener(Self::open_context_menu))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if (this.project, this.connection, this.operation, this.menu_revision) == context_stamp {
                                this.open_menu(None, event.position, window, cx);
                            }
                        }),
                    )
                    .flex()
                    .flex_col()
                    .on_action(cx.listener(Self::up))
                    .on_action(cx.listener(Self::back))
                    .on_action(cx.listener(Self::forward))
                    .on_action(cx.listener(Self::cursor_up))
                    .on_action(cx.listener(Self::cursor_down))
                    .on_action(cx.listener(|this, _: &ToggleHidden, _, cx| {
                        if this.is_loading() {
                            return;
                        }
                        this.show_hidden = !this.show_hidden;
                        this.set_entries(cx);
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &ToggleIgnored, window, cx| {
                        this.toggle_ignored(window, cx);
                    }))
                    .on_action(
                        cx.listener(|this, _: &CursorFirst, w, cx| this.cursor_edge(false, w, cx)),
                    )
                    .on_action(
                        cx.listener(|this, _: &CursorLast, w, cx| this.cursor_edge(true, w, cx)),
                    )
                    .on_action(cx.listener(Self::activate))
                    .on_action(cx.listener(Self::collapse))
                    .on_action(cx.listener(Self::open_directory))
                    .on_action(cx.listener(Self::toggle_mark))
                    .on_action(cx.listener(Self::visual_range))
                    .on_action(cx.listener(Self::mark_all))
                    .on_action(cx.listener(Self::invert_marks))
                    .on_action(cx.listener(Self::clear_marks))
                    .on_action(cx.listener(Self::range_up))
                    .on_action(cx.listener(Self::range_down))
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
                                        let depth = this.tree.node(&path).map_or(0, |n| n.depth);
                                        let expanded =
                                            this.tree.node(&path).is_some_and(|n| n.expanded);
                                        let pending =
                                            this.tree_pending.as_deref() == Some(path.as_str());
                                        let descendants = if entry.directory && !expanded {
                                            this.files.marked_descendants(&path)
                                        } else {
                                            0
                                        };
                                        let toggle_path = path.clone();
                                        let menu_path = path.clone();
                                        let row_stamp = (this.project, this.connection, this.operation, this.menu_revision);
                                        let anchor_path = path.clone();
                                        let row_entity = cx.entity();
                                        let is_directory = entry.directory;
                                        let disclosure = div()
                                            .id(("remote-tree-toggle", index))
                                            .w(metric(size::DISCLOSURE, cx))
                                            .flex_shrink_0()
                                            .child(if pending {
                                                "…"
                                            } else if expanded {
                                                "▾"
                                            } else if is_directory {
                                                "▸"
                                            } else {
                                                " "
                                            })
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                if is_directory {
                                                    cx.stop_propagation();
                                                    if (this.project, this.connection, this.operation, this.menu_revision) == row_stamp {
                                                        this.toggle_tree(toggle_path.clone(), w, cx);
                                                    }
                                                }
                                            }));
                                        #[cfg(test)]
                                        let disclosure = disclosure.test_support();
                                        let name = div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(format!("{}{}", entry.name, if entry.directory { "/" } else { "" }));
                                        #[cfg(test)]
                                        let name = name.id(("remote-row-name", index)).test_support();
                                        // Indentation belongs inside the clipped region, not in the
                                        // row's padding: deep trees must not displace semantic hints.
                                        let name_group = div()
                                            .flex()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .items_center()
                                                    .gap(metric(space::TIGHT, cx))
                                                    .pl(metric(depth as f32 * space::INDENT, cx))
                                                    .child(disclosure)
                                                    .child(if descendants > 0 {
                                                        format!("· {descendants} marked ")
                                                    } else {
                                                        String::new()
                                                    })
                                                    .child(div().w(metric(size::ICON, cx)).flex_shrink_0().child(if this.files.is_marked(&path) { "✓" } else { "" }))
                                                    .child(Icon::new(if entry.directory { if expanded { IconName::FolderOpen } else { IconName::FolderClosed } } else { IconName::File }).size(metric(size::ICON, cx)))
                                                    .child(name),
                                            );
                                        #[cfg(test)]
                                        let name_group = name_group.id(("remote-row-name-group", index)).test_support();
                                        let symlink_text = gpui_kit::StyledText::new("→");
                                        #[cfg(test)]
                                        let symlink_layout = symlink_text.layout().clone();
                                        let symlink = div()
                                            .flex_shrink_0()
                                            .whitespace_nowrap()
                                            .child(symlink_text);
                                        #[cfg(test)]
                                        let symlink = symlink
                                            .on_prepaint(move |_, _, _| assert_eq!(symlink_layout.text(), "→"))
                                            .id(("remote-row-symlink", index)).test_support();
                                        let unmapped_text = gpui_kit::StyledText::new("Unmapped");
                                        #[cfg(test)]
                                        let unmapped_layout = unmapped_text.layout().clone();
                                        let unmapped = div()
                                            .flex_shrink_0()
                                            .whitespace_nowrap()
                                            .text_size(Text::Metadata.rems())
                                            .text_color(palette.muted)
                                            .bg(palette.chrome)
                                            .rounded(metric(size::RADIUS, cx))
                                            .px(metric(space::TIGHT, cx))
                                            .child(unmapped_text);
                                        #[cfg(test)]
                                        let unmapped = unmapped
                                            .on_prepaint(move |_, _, _| assert_eq!(unmapped_layout.text(), "Unmapped"))
                                            .id(("remote-row-unmapped", index)).test_support();
                                        let row = div()
                                            .id(index)
                                            .w_full()
                                            .min_w_0()
                                            .h(metric(size::ROW, cx))
                                            .px(metric(space::PANEL, cx))
                                            .flex()
                                            .items_center()
                                            .gap(metric(space::TIGHT, cx))
                                            .bg(if chosen { accent } else { background })
                                            .hover(move |style| style.bg(if chosen { accent } else { palette.hover }))
                                            .on_prepaint(move |bounds, _, cx| {
                                                row_entity.update(cx, |this, _| {
                                                    if this.files.selected()
                                                        == Some(anchor_path.as_str())
                                                    {
                                                        this.menu_anchor = bounds.bottom_left();
                                                    }
                                                });
                                            })
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    if (this.project, this.connection, this.operation, this.menu_revision) == row_stamp {
                                                        this.open_menu(
                                                            Some(menu_path.clone()),
                                                            event.position,
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                }),
                                            )
                                            .child(name_group)
                                            .when(entry.symlink, |row| row.child(symlink))
                                            .when(!this.files.can_mark(&path), |row| row.child(unmapped))
                                            .on_click(cx.listener(
                                                move |this, event: &gpui_kit::ClickEvent, w, cx| {
                                                    if this.is_loading() || (this.project, this.connection, this.operation, this.menu_revision) != row_stamp
                                                        || this.files.row(index) != Some(path.as_str()) {
                                                        return;
                                                    }
                                                    this.focus.focus(w, cx);
                                                    let modifiers = event.modifiers();
                                                    if modifiers.shift {
                                                        this.files.select_range(index);
                                                    } else if modifiers.control
                                                        || modifiers.platform
                                                    {
                                                        this.files.select(index);
                                                        this.files.toggle_mark();
                                                    } else {
                                                        this.choose(index, w, cx);
                                                        return;
                                                    }
                                                    this.selection_changed(cx);
                                                    cx.notify();
                                                },
                                            ));
                                        #[cfg(test)]
                                        let row = row.test_support();
                                        Some(row)
                                    })
                                    .collect()
                            })
                        })
                        .track_scroll(&self.scroll)
                        .flex_1(),
                    )
                    .children(self.menu.as_ref().map(|(_, menu)| menu.render())),
            )
            .when(self.hosts.is_empty(), |view| {
                view.child(div().p_3().child("Configure a host in Hosts to connect"))
            })
    }
}

#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod visibility_tests;
