//! One independently navigable pane. Finder uses the same virtualized rows.
mod finder;
mod menu;
mod tree;
use crate::actions::*;
use crate::browser_menu::BrowserMenu;
use drift_app::{
    FileList,
    browser::{BrowserService, Directory, Location, OperationId},
    navigation::History,
    tree::FileTree,
};
use drift_core::{local::Entry, project::Registry, store::Store};
use finder::FinderSnapshot;
use gpui_kit::TestSupportExt;
use gpui_kit::base::ElementExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, Render,
    ScrollStrategy, StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle,
    Window, div, point, px, uniform_list,
};
use menu::MenuSnapshot;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum Navigation {
    Reset,
    Push,
    Seek(usize),
    Keep,
}
#[derive(Clone, Copy)]
pub enum BrowserCommand {
    Back,
    Forward,
    Up,
    Refresh,
    Find,
    ReturnFinder,
    Hidden,
    Ignored,
    Cancel,
}
pub enum BrowserEvent {
    Changed(OperationId),
    Opened {
        id: OperationId,
        registry: Registry,
    },
    Selected {
        id: OperationId,
        path: Option<PathBuf>,
        preview: bool,
    },
    Status {
        id: OperationId,
        message: String,
    },
    Compare {
        id: OperationId,
        connection: OperationId,
        path: PathBuf,
    },
    Toolbar {
        id: OperationId,
        connection: OperationId,
        event: crate::toolbar::ToolbarEvent,
    },
}
impl EventEmitter<BrowserEvent> for BrowserPane {}

pub struct BrowserPane {
    filter: Entity<InputState>,
    files: FileList,
    tree: FileTree,
    tree_pending: Option<String>,
    tree_restore: std::collections::VecDeque<String>,
    tree_cursor: Option<String>,
    entries: Vec<Entry>,
    location: Option<Location>,
    store: Store,
    service: BrowserService,
    start: PathBuf,
    generation: u64,
    listing: u64,
    listing_cancel: Option<CancellationToken>,
    show_hidden: bool,
    show_ignored: bool,
    finder: bool,
    finder_snapshot: Option<FinderSnapshot>,
    browser_focus: FocusHandle,
    history: History,
    navigation: Navigation,
    scroll: UniformListScrollHandle,
    menu: Option<(MenuSnapshot, BrowserMenu)>,
    menu_anchor: Point<Pixels>,
    menu_serial: u64,
    menu_revision: u64,
    comparison_connection: Option<OperationId>,
    _subscription: Subscription,
}
impl Focusable for BrowserPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.browser_focus.clone()
    }
}
impl Drop for BrowserPane {
    fn drop(&mut self) {
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
    }
}
impl BrowserPane {
    pub fn new(
        store: Store,
        service: BrowserService,
        start: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files…"));
        let subscription = cx.subscribe_in(&filter, window, |this, state, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.dismiss_context_menu(window, cx);
                this.menu_revision += 1;
                if this.finder {
                    this.files.filter_finder(&state.read(cx).value());
                } else {
                    this.files.filter(&state.read(cx).value());
                }
                this.selection_changed(false, cx);
                this.status(format!("{} entries", this.files.len()), cx);
                cx.notify();
            }
        });
        Self {
            filter,
            files: FileList::new(vec![]),
            tree: FileTree::default(),
            tree_pending: None,
            tree_restore: Default::default(),
            tree_cursor: None,
            entries: vec![],
            location: None,
            store,
            service,
            start,
            generation: 0,
            listing: 0,
            listing_cancel: None,
            show_hidden: false,
            show_ignored: false,
            finder: false,
            finder_snapshot: None,
            browser_focus: cx.focus_handle().tab_stop(true),
            history: History::default(),
            navigation: Navigation::Reset,
            scroll: UniformListScrollHandle::new(),
            menu: None,
            menu_anchor: point(px(0.), px(0.)),
            menu_serial: 0,
            menu_revision: 0,
            comparison_connection: None,
            _subscription: subscription,
        }
    }
    pub fn id(&self) -> OperationId {
        OperationId {
            project: self.generation,
            operation: self.listing,
        }
    }
    pub fn location(&self) -> Option<&Location> {
        self.location.as_ref()
    }
    pub fn is_loading(&self) -> bool {
        self.listing_cancel.is_some()
    }
    pub fn len(&self) -> usize {
        self.files.len()
    }
    pub fn marked(&self) -> Vec<String> {
        self.files.marked()
    }
    pub fn selected(&self) -> Option<&str> {
        self.files.selected()
    }
    pub fn finder_active(&self) -> bool {
        self.finder
    }
    pub fn can_back(&self) -> bool {
        self.finder || self.history.previous().is_some()
    }
    pub fn can_forward(&self) -> bool {
        self.history.next().is_some()
    }
    pub fn can_up(&self) -> bool {
        self.location
            .as_ref()
            .and_then(Location::parent_directory)
            .is_some()
    }
    pub fn shows_hidden(&self) -> bool {
        self.show_hidden
    }
    pub fn shows_ignored(&self) -> bool {
        self.show_ignored
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.browser_focus.focus(window, cx);
    }
    pub fn focus_filter_input(&self, window: &mut Window, cx: &mut App) {
        self.filter.focus_handle(cx).focus(window, cx);
    }
    pub fn command(
        &mut self,
        command: BrowserCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            BrowserCommand::Back => self.go_back(&GoBack, window, cx),
            BrowserCommand::Forward => self.go_forward(&GoForward, window, cx),
            BrowserCommand::Up => self.up(window, cx),
            BrowserCommand::Refresh => self.reload(window, cx),
            BrowserCommand::Find => self.find(window, cx),
            BrowserCommand::ReturnFinder => self.return_finder(window, cx),
            BrowserCommand::Hidden => {
                self.show_hidden = !self.show_hidden;
                self.reload(window, cx);
            }
            BrowserCommand::Ignored => {
                self.show_ignored = !self.show_ignored;
                self.reload(window, cx);
            }
            BrowserCommand::Cancel => self.cancel(window, cx),
        }
    }
    fn status(&self, message: String, cx: &mut Context<Self>) {
        cx.emit(BrowserEvent::Status {
            id: self.id(),
            message,
        });
    }
    fn selection_changed(&self, preview: bool, cx: &mut Context<Self>) {
        cx.emit(BrowserEvent::Selected {
            id: self.id(),
            path: self.files.selected().map(PathBuf::from),
            preview,
        });
    }
    pub fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.request_open(path, Navigation::Reset, window, cx);
    }
    fn request_open(
        &mut self,
        path: PathBuf,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.return_finder(window, cx);
        self.browser_focus.focus(window, cx);
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.generation += 1;
        self.listing += 1;
        self.tree_pending = None;
        self.selection_changed(false, cx);
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        self.navigation = navigation;
        cx.emit(BrowserEvent::Changed(self.id()));
        let id = OperationId {
            project: self.generation,
            operation: self.listing,
        };
        let operation = self.service.open(
            self.store.clone(),
            path,
            id,
            self.show_hidden,
            self.show_ignored,
        );
        self.listing_cancel = Some(operation.cancel);
        self.status("Opening project…".into(), cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != id.project || this.listing != id.operation {
                    return;
                }
                this.listing_cancel = None;
                match result {
                    Ok(Ok(directory)) => this.apply_directory(id, directory, window, cx),
                    Ok(Err(error)) => this.status(error.to_string(), cx),
                    Err(error) => this.status(format!("Background task failed: {error}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn apply_directory(
        &mut self,
        id: OperationId,
        directory: Directory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id.project != self.generation || id.operation != self.listing {
            return;
        }
        match self.navigation {
            Navigation::Reset => self.history.reset(directory.location.directory.clone()),
            Navigation::Push => self.history.push(directory.location.directory.clone()),
            Navigation::Seek(index) => self.history.commit(index, &directory.location.directory),
            Navigation::Keep => {}
        }
        self.finder = false;
        self.finder_snapshot = None;
        if !matches!(self.navigation, Navigation::Keep) {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        if self.location.as_ref().is_none_or(|old| {
            old.root.base() != directory.location.root.base() || old.slug != directory.location.slug
        }) {
            self.files = FileList::new(vec![]);
        }
        self.location = Some(directory.location);
        cx.emit(BrowserEvent::Opened {
            id,
            registry: directory.registry,
        });
        self.set_entries(
            directory.entries,
            matches!(self.navigation, Navigation::Keep),
            cx,
        );
        self.restore_tree(window, cx);
        self.selection_changed(false, cx);
        cx.notify();
    }
    fn set_entries(&mut self, entries: Vec<Entry>, keep_expanded: bool, cx: &mut Context<Self>) {
        let nodes = entries
            .iter()
            .map(|e| (e.path.to_string_lossy().into_owned(), e.directory));
        if keep_expanded {
            self.tree.reload(nodes);
        } else {
            self.tree = FileTree::new(nodes);
        }
        self.entries = entries;
        self.rebuild_tree(cx);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.status(format!("{} entries", self.files.len()), cx);
    }
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.location.clone() else {
            self.open(self.start.clone(), window, cx);
            return;
        };
        self.read_location(location, Navigation::Keep, window, cx);
    }
    fn read_location(
        &mut self,
        location: Location,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.return_finder(window, cx);
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
        self.navigation = navigation;
        self.listing += 1;
        self.tree_pending = None;
        cx.emit(BrowserEvent::Changed(self.id()));
        self.selection_changed(false, cx);
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        let id = OperationId {
            project: self.generation,
            operation: self.listing,
        };
        let operation = self.service.read_directory(
            self.store.clone(),
            location,
            id,
            self.show_hidden,
            self.show_ignored,
        );
        self.listing_cancel = Some(operation.cancel);
        self.status("Loading directory…".into(), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != id.project || this.listing != id.operation {
                    return;
                }
                this.listing_cancel = None;
                match result {
                    Ok(Ok(directory)) => this.apply_directory(id, directory, window, cx),
                    Ok(Err(e)) => this.status(e.to_string(), cx),
                    Err(e) => this.status(e.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.files.select(index);
        let Some(path) = self.files.selected().map(PathBuf::from) else {
            return;
        };
        let Some(location) = self.location.clone() else {
            return;
        };
        let Some(entry) = self.entries.iter().find(|e| e.path == path) else {
            return;
        };
        if entry.directory {
            self.navigate(
                location.root.base().join(&path),
                Navigation::Push,
                window,
                cx,
            );
            return;
        }
        self.selection_changed(true, cx);
        cx.notify();
    }
    pub fn preview_selected(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading() || self.location.is_none() {
            return;
        }
        let Some(path) = self.files.selected() else {
            return;
        };
        if self.entries.iter().any(|entry| {
            entry.path == std::path::Path::new(path) && !entry.directory && !entry.symlink
        }) {
            self.selection_changed(true, cx);
        }
    }
    fn up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = self.location.as_ref().and_then(Location::parent_directory) {
            self.navigate(parent, Navigation::Push, window, cx);
        }
    }
    fn navigate(
        &mut self,
        path: PathBuf,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut location) = self.location.clone() else {
            return;
        };
        self.browser_focus.focus(window, cx);
        if path.starts_with(location.root.base()) {
            location.directory = path;
            self.read_location(location, navigation, window, cx);
        } else if location.slug.is_none() {
            // Explicit navigation outside an unregistered folder opens a new
            // capability root. Registered project roots keep their boundary.
            self.request_open(path, navigation, window, cx);
        }
    }
    fn go_up(&mut self, _: &GoUp, window: &mut Window, cx: &mut Context<Self>) {
        self.up(window, cx);
    }
    fn go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        if self.finder {
            self.return_finder(window, cx);
            return;
        }
        if let Some((index, path)) = self.history.previous() {
            self.navigate(path, Navigation::Seek(index), window, cx);
        }
    }
    fn go_forward(&mut self, _: &GoForward, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, path)) = self.history.next() {
            self.navigate(path, Navigation::Seek(index), window, cx);
        }
    }
    fn cursor_edge(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let row = if last {
            self.files.len().saturating_sub(1)
        } else {
            0
        };
        self.files.select(row);
        self.selection_changed(false, cx);
        self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        cx.notify();
    }
    fn cursor_up(&mut self, _: &CursorUp, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.files
            .select(self.files.selected_row().unwrap_or(0).saturating_sub(1));
        self.selection_changed(false, cx);
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn cursor_down(&mut self, _: &CursorDown, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let index = self
            .files
            .selected_row()
            .map_or(0, |i| (i + 1).min(self.files.len().saturating_sub(1)));
        self.files.select(index);
        self.selection_changed(false, cx);
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        cx.notify();
    }
    fn toggle_mark(&mut self, _: &ToggleMark, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
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
        if !self.browser_focus.is_focused(window) {
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
        if !self.browser_focus.is_focused(window) {
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
        if !self.browser_focus.is_focused(window) {
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
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        if self.files.range_active() {
            self.files.clear_marks();
        } else if !self.filter.read(cx).value().is_empty() {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
            // Programmatic input changes do not emit InputEvent::Change.
            if self.finder {
                self.files.filter_finder("");
            } else {
                self.files.filter("");
            }
            self.selection_changed(false, cx);
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
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let index = self.files.selected_row().unwrap_or(0).saturating_sub(1);
        self.files.select_range(index);
        self.selection_changed(false, cx);
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn range_down(&mut self, _: &RangeDown, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let index = self
            .files
            .selected_row()
            .map_or(0, |i| (i + 1).min(self.files.len().saturating_sub(1)));
        self.files.select_range(index);
        self.selection_changed(false, cx);
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn open_directory(&mut self, _: &OpenDirectory, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.choose(self.files.selected_row().unwrap_or(0), window, cx);
    }
    fn activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
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
                self.selection_changed(false, cx);
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
    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.focus_handle(cx).focus(window, cx);
    }
    pub fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = self.files.selected() {
            cx.write_to_clipboard(ClipboardItem::new_string(name.into()));
            self.status(format!("Copied {name}"), cx);
            cx.notify();
        }
    }
    pub fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        self.reload(window, cx);
    }
    pub fn stop_listing(&mut self, cx: &mut Context<Self>) {
        self.listing += 1;
        self.tree_pending = None;
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        self.selection_changed(false, cx);
        cx.emit(BrowserEvent::Changed(self.id()));
        cx.notify();
    }
    /// A moved/removed active registry entry invalidates its old capability root.
    pub fn invalidate_project(&mut self, cx: &mut Context<Self>) {
        self.finder = false;
        self.finder_snapshot = None;
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.generation += 1;
        self.location = None;
        self.entries.clear();
        self.tree = FileTree::default();
        self.files = FileList::new(vec![]);
        self.history = History::default();
        self.stop_listing(cx);
    }
    fn cancel(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.stop_listing(cx);
        self.status("Cancelled".into(), cx);
    }
    pub fn replace_config(
        &mut self,
        config: drift_core::config::RuntimeConfig,
        cx: &mut Context<Self>,
    ) {
        self.menu_revision += 1;
        if let Some(location) = &mut self.location {
            location.config = config;
        }
        cx.notify();
    }
    pub fn header(&self) -> gpui_kit::AnyElement {
        let path = self
            .location
            .as_ref()
            .map(|l| l.directory.display().to_string())
            .unwrap_or_else(|| self.start.display().to_string());
        div()
            .flex()
            .min_w_0()
            .gap_3()
            .p_3()
            .child(
                div()
                    .w(px(300.))
                    .min_w_0()
                    .flex_shrink_1()
                    .child(crate::form_input::guard(&self.filter, |input| {
                        input.id("filter").w_full().min_w_0().flex_shrink_1()
                    })),
            )
            .child(div().flex_1().min_w_0().truncate().child(path))
            .into_any_element()
    }
}
impl Render for BrowserPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.validate_menu(window, cx);
        let background = cx.theme().background;
        let accent = cx.theme().accent;
        let border = cx.theme().border;
        let count = self.files.len();
        let entity = cx.entity();
        let context_stamp = (self.id(), self.menu_revision);
        div()
            .id("browser-pane")
            .key_context("DriftBrowser")
            .track_focus(&self.browser_focus)
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::go_up))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::go_forward))
            .on_action(cx.listener(Self::cursor_up))
            .on_action(cx.listener(Self::cursor_down))
            .on_action(cx.listener(|this, _: &ToggleHidden, w, cx| {
                this.command(BrowserCommand::Hidden, w, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleIgnored, w, cx| {
                this.command(BrowserCommand::Ignored, w, cx)
            }))
            .on_action(cx.listener(|this, _: &CursorFirst, w, cx| this.cursor_edge(false, w, cx)))
            .on_action(cx.listener(|this, _: &CursorLast, w, cx| this.cursor_edge(true, w, cx)))
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
            .on_action(cx.listener(Self::keyboard_menu))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .border_r_1()
            .border_color(border)
            .child(div().flex().flex_wrap().min_w_0().items_center().gap_3().p_3().border_b_1().border_color(border).children(self.finder.then(|| {
                Button::new("finder-return")
                    .label("Return")
                    .min_w_0()
                    .max_w_full()
                    .overflow_hidden()
                    .on_click(cx.listener(|this, _, w, cx| this.return_finder(w, cx)))
            })).child(format!(
                "{} · {} marked{}",
                if self.finder {
                    "Project files"
                } else {
                    "Local"
                },
                self.files.marked().len(),
                if self.files.range_active() {
                    " · range: press v to finish"
                } else {
                    ""
                }
            )))
            .child(
                div()
                .id("local-files-pane")
                .test_support()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if (this.id(), this.menu_revision) == context_stamp {
                        this.open_context_menu(None, event.position, window, cx);
                    }
                }))
                .on_prepaint({
                    let pane = cx.weak_entity();
                    move |bounds, _, cx| {
                        let _ = pane.update(cx, |this, _| {
                            if this.files.selected().is_none() {
                                this.menu_anchor = point(bounds.origin.x + px(12.), bounds.origin.y + px(12.));
                            }
                        });
                    }
                })
                .child(uniform_list("local-files", count, move |range, _, cx| {
                    entity.update(cx, |this, cx| {
                        range
                            .filter_map(|index| {
                                let name = this.files.row(index)?.to_owned();
                                let chosen = this.files.selected() == Some(name.as_str());
                                let directory = this
                                    .entries
                                    .iter()
                                    .find(|e| e.path.to_string_lossy() == name)
                                    .is_some_and(|e| e.directory);
                                let display_name = if this.finder {
                                    name.clone()
                                } else {
                                    std::path::Path::new(&name)
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or(name.clone())
                                };
                                let label = format!(
                                    "{} {}{}",
                                    if this.files.is_marked(&name) {
                                        "✓"
                                    } else {
                                        " "
                                    },
                                    display_name,
                                    if directory { "/" } else { "" }
                                );
                                let depth = this.tree.node(&name).map_or(0, |n| n.depth);
                                let expanded = this.tree.node(&name).is_some_and(|n| n.expanded);
                                let pending = this.tree_pending.as_deref() == Some(name.as_str());
                                let descendants = if directory && !expanded {
                                    this.files.marked_descendants(&name)
                                } else {
                                    0
                                };
                                let toggle_path = name.clone();
                                let is_directory = directory;
                                let disclosure = div()
                                    .id(("local-tree-toggle", index))
                                    .w(px(22.))
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
                                            this.toggle_tree(toggle_path.clone(), w, cx);
                                        }
                                    }));
                                #[cfg(test)]
                                let disclosure = disclosure.test_support();
                                let context_path = name.clone();
                                let row_stamp = (this.id(), this.menu_revision);
                                let anchor_path = name;
                                let anchor_owner = cx.weak_entity();
                                let row = div()
                                    .id(index)
                                    .h(px(28.))
                                    .pr_3()
                                    .pl(px(12. + depth as f32 * 16.))
                                    .flex()
                                    .items_center()
                                    .bg(if chosen { accent } else { background })
                                    .child(disclosure)
                                    .child(label)
                                    .child(if descendants > 0 {
                                        format!(" · {descendants} marked")
                                    } else {
                                        String::new()
                                    })
                                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &gpui_kit::MouseDownEvent, w, cx| {
                                        cx.stop_propagation();
                                        if (this.id(), this.menu_revision) == row_stamp {
                                            this.open_context_menu(Some(context_path.clone()), event.position, w, cx);
                                        }
                                    }))
                                    .on_prepaint(move |bounds, _, cx| {
                                        let _ = anchor_owner.update(cx, |this, _| {
                                            if this.files.selected() == Some(anchor_path.as_str()) {
                                                this.menu_anchor = point(bounds.origin.x + px(22.), bounds.origin.y + bounds.size.height);
                                            }
                                        });
                                    })
                                    .on_click(cx.listener(
                                        move |this, event: &gpui_kit::ClickEvent, w, cx| {
                                            this.browser_focus.focus(w, cx);
                                            let modifiers = event.modifiers();
                                            if modifiers.shift {
                                                this.files.select_range(index);
                                            } else if modifiers.control || modifiers.platform {
                                                this.files.select(index);
                                                this.files.toggle_mark();
                                            } else {
                                                this.choose(index, w, cx);
                                                return;
                                            }
                                            this.selection_changed(false, cx);
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
                .flex_1())
                .children(self.menu.as_ref().map(|(_, menu)| menu.render())),
            )
    }
}

#[cfg(test)]
mod finder_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod tests;
