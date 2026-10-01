use crate::hosts::{HostEvent, HostManager};
use drift_app::{
    FileList,
    browser::{BrowserService, Directory, Location, OperationId},
    hosts::{HostCommand, HostResponse},
    navigation::History,
};
use drift_core::{local::Entry, project::Registry, store::Store};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{Editor, EditorState, Input, InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder;
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
use gpui_kit::{
    App, AppContext, ClipboardItem, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, PathPromptOptions, Render, ScrollStrategy,
    StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

gpui_kit::actions!(
    drift,
    [
        FocusFilter,
        CopySelection,
        Refresh,
        Cancel,
        GoUp,
        GoBack,
        GoForward,
        CursorUp,
        CursorDown,
        Activate
    ]
);
pub fn bind_keys(cx: &mut App) {
    crate::hosts::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("ctrl-f", FocusFilter, Some("Drift")),
        KeyBinding::new("cmd-f", FocusFilter, Some("Drift")),
        KeyBinding::new("ctrl-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("cmd-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("f5", Refresh, Some("Drift")),
        KeyBinding::new("escape", Cancel, Some("Drift")),
        KeyBinding::new("alt-left", GoBack, Some("DriftBrowser")),
        KeyBinding::new("alt-right", GoForward, Some("DriftBrowser")),
        KeyBinding::new("alt-up", GoUp, Some("DriftBrowser")),
        KeyBinding::new("backspace", GoUp, Some("DriftBrowser")),
        KeyBinding::new("left", GoUp, Some("DriftBrowser")),
        KeyBinding::new("h", GoUp, Some("DriftBrowser")),
        KeyBinding::new("up", CursorUp, Some("DriftBrowser")),
        KeyBinding::new("k", CursorUp, Some("DriftBrowser")),
        KeyBinding::new("down", CursorDown, Some("DriftBrowser")),
        KeyBinding::new("j", CursorDown, Some("DriftBrowser")),
        KeyBinding::new("enter", Activate, Some("DriftBrowser")),
        KeyBinding::new("right", Activate, Some("DriftBrowser")),
        KeyBinding::new("l", Activate, Some("DriftBrowser")),
    ]);
}
#[derive(Clone, Copy)]
enum Navigation {
    Reset,
    Push,
    Seek(usize),
    Keep,
}
pub struct Shell {
    hosts: Option<Entity<HostManager>>,
    host_subscription: Option<Subscription>,
    configuration: u64,
    config_cancel: Option<CancellationToken>,
    filter: Entity<InputState>,
    preview: Entity<EditorState>,
    name: Entity<InputState>,
    project_query: Entity<InputState>,
    files: FileList,
    entries: Vec<Entry>,
    location: Option<Location>,
    registry: Registry,
    store: Store,
    service: BrowserService,
    start: PathBuf,
    generation: u64,
    listing: u64,
    preview_generation: u64,
    listing_cancel: Option<CancellationToken>,
    preview_cancel: Option<CancellationToken>,
    status: String,
    preview_title: String,
    preview_text: String,
    show_hidden: bool,
    show_ignored: bool,
    finder: bool,
    projects: bool,
    browser_focus: FocusHandle,
    history: History,
    navigation: Navigation,
    scroll: UniformListScrollHandle,
    folder_prompt: bool,
    _subscription: Subscription,
    _project_subscription: Subscription,
}
impl Drop for Shell {
    fn drop(&mut self) {
        if let Some(cancel) = self.config_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancel) = &self.listing_cancel {
            cancel.cancel();
        }
        if let Some(cancel) = &self.preview_cancel {
            cancel.cancel();
        }
    }
}
impl Shell {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        store: Store,
        service: BrowserService,
        start: PathBuf,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files…"));
        let preview = cx.new(|cx| {
            EditorState::new(window, cx)
                .line_number(true)
                .soft_wrap(true)
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        let project_query = cx.new(|cx| InputState::new(window, cx).placeholder("Find a project…"));
        let project_subscription =
            cx.subscribe_in(&project_query, window, |_, _, _: &InputEvent, _, cx| {
                cx.notify()
            });
        let subscription = cx.subscribe_in(&filter, window, |this, state, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.files.filter(&state.read(cx).value());
                this.clear_preview(window, cx);
                this.status = format!("{} entries", this.files.len());
                cx.notify();
            }
        });
        let browser_focus = cx.focus_handle().tab_stop(true);
        browser_focus.focus(window, cx);
        let mut shell = Self {
            hosts: None,
            host_subscription: None,
            configuration: 0,
            config_cancel: None,
            filter,
            preview,
            name,
            project_query,
            files: FileList::new(vec![]),
            entries: vec![],
            location: None,
            registry: Registry::default(),
            store,
            service,
            start: start.clone(),
            generation: 0,
            listing: 0,
            preview_generation: 0,
            listing_cancel: None,
            preview_cancel: None,
            status: "Opening project…".into(),
            preview_title: "Preview".into(),
            preview_text: String::new(),
            show_hidden: false,
            show_ignored: false,
            finder: false,
            projects: false,
            browser_focus,
            history: History::default(),
            navigation: Navigation::Reset,
            scroll: UniformListScrollHandle::new(),
            folder_prompt: false,
            _subscription: subscription,
            _project_subscription: project_subscription,
        };
        shell.open(start, window, cx);
        shell
    }
    fn clear_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview_generation += 1;
        if let Some(cancel) = self.preview_cancel.take() {
            cancel.cancel();
        }
        self.preview_text.clear();
        self.preview_title = "Preview".into();
        self.preview
            .update(cx, |state, cx| state.set_value("", window, cx));
    }
    fn open_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slug = self.location.as_ref().and_then(|l| l.slug.clone());
        let hosts = cx
            .new(|cx| HostManager::new(self.store.clone(), self.service.clone(), slug, window, cx));
        self.host_subscription =
            Some(
                cx.subscribe_in(&hosts, window, |this, _, event: &HostEvent, window, cx| {
                    match event {
                        HostEvent::Close => {
                            this.hosts = None;
                            this.host_subscription = None;
                            this.browser_focus.focus(window, cx);
                        }
                        HostEvent::Changed => this.refresh_config(window, cx),
                    }
                    cx.notify();
                }),
            );
        self.hosts = Some(hosts);
        cx.notify();
    }
    fn refresh_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = &self.location else {
            return;
        };
        self.configuration += 1;
        if let Some(cancel) = self.config_cancel.take() {
            cancel.cancel();
        }
        let id = OperationId {
            project: self.generation,
            operation: self.configuration,
        };
        let operation = self.service.manage_hosts(
            self.store.clone(),
            location.slug.clone(),
            HostCommand::Load,
            id,
        );
        self.config_cancel = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.generation != id.project || this.configuration != id.operation {
                    return;
                }
                this.config_cancel = None;
                match result {
                    Ok(Ok(HostResponse::Loaded(catalog))) => {
                        if let Some(location) = &mut this.location {
                            location.config = catalog.runtime;
                        }
                    }
                    Ok(Ok(_)) => unreachable!("configuration load only returns a catalog"),
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Configuration refresh failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.request_open(path, Navigation::Reset, window, cx);
    }
    fn request_open(
        &mut self,
        path: PathBuf,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configuration += 1;
        if let Some(cancel) = self.config_cancel.take() {
            cancel.cancel();
        }
        self.browser_focus.focus(window, cx);
        self.generation += 1;
        self.listing += 1;
        self.clear_preview(window, cx);
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        self.navigation = navigation;
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
        self.status = "Opening project…".into();
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
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Background task failed: {error}"),
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
        if !matches!(self.navigation, Navigation::Keep) {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        self.location = Some(directory.location);
        self.registry = directory.registry;
        self.set_entries(directory.entries, cx);
        self.clear_preview(window, cx);
        cx.notify();
    }
    fn set_entries(&mut self, entries: Vec<Entry>, cx: &mut Context<Self>) {
        self.entries = entries;
        self.files = FileList::new(
            self.entries
                .iter()
                .map(|e| e.path.to_string_lossy().into_owned())
                .collect(),
        );
        self.files.filter(&self.filter.read(cx).value());
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.status = format!("{} entries", self.files.len());
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
        self.navigation = navigation;
        self.listing += 1;
        self.clear_preview(window, cx);
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
        self.status = "Loading directory…".into();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != id.project || this.listing != id.operation {
                    return;
                }
                this.listing_cancel = None;
                match result {
                    Ok(Ok(directory)) => this.apply_directory(id, directory, window, cx),
                    Ok(Err(e)) => this.status = e.to_string(),
                    Err(e) => this.status = e.to_string(),
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
        self.clear_preview(window, cx);
        self.preview_title = path.display().to_string();
        let id = OperationId {
            project: self.generation,
            operation: self.preview_generation,
        };
        let operation = self.service.preview(location, path, id);
        self.preview_cancel = Some(operation.cancel);
        self.status = "Loading preview…".into();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != id.project || this.preview_generation != id.operation {
                    return;
                }
                this.preview_cancel = None;
                match result {
                    Ok(Ok(text)) => {
                        this.preview
                            .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
                        this.preview_text = text;
                        this.status = format!("{} entries", this.files.len());
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
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
        if let Some((index, path)) = self.history.previous() {
            self.navigate(path, Navigation::Seek(index), window, cx);
        }
    }
    fn go_forward(&mut self, _: &GoForward, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, path)) = self.history.next() {
            self.navigate(path, Navigation::Seek(index), window, cx);
        }
    }
    fn cursor_up(&mut self, _: &CursorUp, window: &mut Window, cx: &mut Context<Self>) {
        self.files
            .select(self.files.selected_row().unwrap_or(0).saturating_sub(1));
        self.clear_preview(window, cx);
        self.scroll.scroll_to_item(
            self.files.selected_row().unwrap_or(0),
            ScrollStrategy::Nearest,
        );
        cx.notify();
    }
    fn cursor_down(&mut self, _: &CursorDown, window: &mut Window, cx: &mut Context<Self>) {
        let index = self
            .files
            .selected_row()
            .map_or(0, |i| (i + 1).min(self.files.len().saturating_sub(1)));
        self.files.select(index);
        self.clear_preview(window, cx);
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        cx.notify();
    }
    fn activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        self.choose(self.files.selected_row().unwrap_or(0), window, cx);
    }
    fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open folder".into()),
        });
        self.folder_prompt = true;
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.folder_prompt = false;
                if this.generation == generation {
                    match result {
                        Ok(Ok(Some(paths))) => {
                            if let Some(path) = paths.into_iter().next() {
                                this.open(path, window, cx);
                            }
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => this.status = error.to_string(),
                        Err(error) => this.status = format!("Folder picker failed: {error}"),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.location.clone() else {
            return;
        };
        self.listing += 1;
        self.clear_preview(window, cx);
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        let id = OperationId {
            project: self.generation,
            operation: self.listing,
        };
        let operation = self
            .service
            .find(location, id, self.show_hidden, self.show_ignored);
        self.listing_cancel = Some(operation.cancel);
        self.status = "Finding project files…".into();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.generation != id.project || this.listing != id.operation {
                    return;
                }
                this.listing_cancel = None;
                match result {
                    Ok(Ok(entries)) => {
                        this.finder = true;
                        this.set_entries(entries, cx);
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn register(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.location.clone() else {
            return;
        };
        let name = self.name.read(cx).value().to_string();
        if name.trim().is_empty() {
            self.status = "Enter a project name".into();
            cx.notify();
            return;
        }
        self.listing += 1;
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        let id = OperationId {
            project: self.generation,
            operation: self.listing,
        };
        let operation = self
            .service
            .register(self.store.clone(), name, location.directory, id);
        self.listing_cancel = Some(operation.cancel);
        self.status = "Registering project…".into();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != id.project || this.listing != id.operation {
                    return;
                }
                this.listing_cancel = None;
                match result {
                    Ok(Ok(path)) => {
                        this.projects = false;
                        this.open(path, window, cx);
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.focus_handle(cx).focus(window, cx);
    }
    fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = self.files.selected() {
            cx.write_to_clipboard(ClipboardItem::new_string(name.into()));
            self.status = format!("Copied {name}");
            cx.notify();
        }
    }
    fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        self.reload(window, cx);
    }
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects {
            self.projects = false;
            self.browser_focus.focus(window, cx);
            cx.notify();
            return;
        }
        self.listing += 1;
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        self.clear_preview(window, cx);
        self.status = "Cancelled".into();
        cx.notify();
    }
}
impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(hosts) = &self.hosts {
            return div().size_full().child(hosts.clone()).into_any_element();
        }
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        let accent = cx.theme().accent;
        let entity = cx.entity();
        let count = self.files.len();
        let path = self
            .location
            .as_ref()
            .map(|l| l.directory.display().to_string())
            .unwrap_or_else(|| self.start.display().to_string());
        let hosts = self
            .location
            .as_ref()
            .map(|l| {
                l.config
                    .hosts
                    .iter()
                    .map(|h| {
                        format!(
                            "{} ({})",
                            h.name,
                            if h.protocol.is_empty() {
                                "sftp"
                            } else {
                                &h.protocol
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        div()
            .key_context("Drift")
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cancel))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .p_3()
                    .border_b_1()
                    .border_color(border)
                    .child(div().font_weight(gpui_kit::FontWeight::BOLD).child("drift"))
                    .child(
                        Button::new("projects")
                            .label("Projects")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.projects = !this.projects;
                                if this.projects {
                                    this.project_query.focus_handle(cx).focus(window, cx);
                                } else {
                                    this.browser_focus.focus(window, cx);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("hosts")
                            .label("Hosts")
                            .disabled(self.location.is_none() || self.listing_cancel.is_some())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_hosts(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("open-folder")
                            .label("Open folder")
                            .disabled(self.folder_prompt)
                            .on_click(cx.listener(|this, _, w, cx| this.open_folder(w, cx))),
                    )
                    .child(
                        Button::new("back")
                            .label("Back")
                            .disabled(self.history.previous().is_none())
                            .on_click(cx.listener(|this, _, w, cx| this.go_back(&GoBack, w, cx))),
                    )
                    .child(
                        Button::new("forward")
                            .label("Forward")
                            .disabled(self.history.next().is_none())
                            .on_click(
                                cx.listener(|this, _, w, cx| this.go_forward(&GoForward, w, cx)),
                            ),
                    )
                    .child(
                        Button::new("up")
                            .label("Up")
                            .disabled(
                                self.location
                                    .as_ref()
                                    .and_then(Location::parent_directory)
                                    .is_none(),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.up(w, cx))),
                    )
                    .child(
                        Button::new("refresh")
                            .label("Refresh")
                            .on_click(cx.listener(|this, _, w, cx| this.reload(w, cx))),
                    )
                    .child(
                        Button::new("cancel")
                            .label("Cancel")
                            .disabled(
                                self.listing_cancel.is_none() && self.preview_cancel.is_none(),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.cancel(&Cancel, w, cx))),
                    )
                    .child(
                        Button::new("find")
                            .label("Find files")
                            .disabled(self.location.is_none())
                            .on_click(cx.listener(|this, _, w, cx| this.find(w, cx))),
                    )
                    .child(
                        Button::new("hidden")
                            .label(if self.show_hidden {
                                "Hide hidden"
                            } else {
                                "Show hidden"
                            })
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.show_hidden = !this.show_hidden;
                                this.reload(w, cx);
                            })),
                    )
                    .child(
                        Button::new("ignored")
                            .label(if self.show_ignored {
                                "Hide ignored"
                            } else {
                                "Show ignored"
                            })
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.show_ignored = !this.show_ignored;
                                this.reload(w, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .p_3()
                    .child(Input::new(&self.filter).id("filter").w(px(300.)))
                    .child(path),
            )
            .when(self.projects, |view| {
                let query = self.project_query.read(cx).value().to_lowercase();
                view.child(
                    div()
                        .flex()
                        .flex_col()
                        .p_3()
                        .gap_2()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            Input::new(&self.project_query)
                                .id("project-filter")
                                .w(px(320.)),
                        )
                        .children(
                            self.registry
                                .active()
                                .into_iter()
                                .filter(|p| {
                                    p.name.to_lowercase().contains(&query)
                                        || p.slug.to_lowercase().contains(&query)
                                })
                                .map(|p| {
                                    let path = p.path.clone();
                                    Button::new(p.slug.clone())
                                        .label(p.name.clone())
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.projects = false;
                                            this.open(path.clone(), w, cx);
                                        }))
                                        .into_any_element()
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(Input::new(&self.name).id("project-name").w(px(280.)))
                                .child(
                                    Button::new("register")
                                        .label("Register current folder")
                                        .disabled(
                                            self.location.as_ref().is_none_or(|l| l.slug.is_some()),
                                        )
                                        .on_click(
                                            cx.listener(|this, _, w, cx| this.register(w, cx)),
                                        ),
                                ),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("browser-pane")
                            .key_context("DriftBrowser")
                            .track_focus(&self.browser_focus)
                            .on_action(cx.listener(Self::go_up))
                            .on_action(cx.listener(Self::go_back))
                            .on_action(cx.listener(Self::go_forward))
                            .on_action(cx.listener(Self::cursor_up))
                            .on_action(cx.listener(Self::cursor_down))
                            .on_action(cx.listener(Self::activate))
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .border_r_1()
                            .border_color(border)
                            .child(div().p_3().border_b_1().border_color(border).child(
                                if self.finder {
                                    "Project files"
                                } else {
                                    "Local"
                                },
                            ))
                            .child(
                                uniform_list("local-files", count, move |range, _, cx| {
                                    entity.update(cx, |this, cx| {
                                        range
                                            .filter_map(|index| {
                                                let name = this.files.row(index)?.to_owned();
                                                let chosen =
                                                    this.files.selected() == Some(name.as_str());
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
                                                    "{}{}",
                                                    display_name,
                                                    if directory { "/" } else { "" }
                                                );
                                                let row = div()
                                                    .id(index)
                                                    .h(px(28.))
                                                    .px_3()
                                                    .flex()
                                                    .items_center()
                                                    .bg(if chosen { accent } else { background })
                                                    .child(label)
                                                    .on_click(cx.listener(
                                                        move |this, _, w, cx| {
                                                            this.browser_focus.focus(w, cx);
                                                            this.choose(index, w, cx)
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
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .p_3()
                                    .border_b_1()
                                    .border_color(border)
                                    .child(self.preview_title.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .p_2()
                                    .gap_2()
                                    .child(
                                        Button::new("copy-selection")
                                            .label("Copy path")
                                            .disabled(self.files.selected().is_none())
                                            .on_click(cx.listener(|this, _, w, cx| {
                                                this.copy_selection(&CopySelection, w, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("copy-preview")
                                            .label("Copy text")
                                            .disabled(self.preview_text.is_empty())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    this.preview_text.clone(),
                                                ))
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("preview-pane")
                                    .flex_1()
                                    .min_h_0()
                                    .child(Editor::new(&self.preview).readonly(true).size_full()),
                            )
                            .child(div().p_2().child(if hosts.is_empty() {
                                "No hosts configured".into()
                            } else {
                                format!("Hosts: {hosts}")
                            })),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(border)
                    .child(self.status.clone()),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drift_core::{config::RuntimeConfig, local::ProjectRoot};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
    use std::{fs, sync::Arc};

    #[gpui_kit::test]
    fn real_directory_filtering_and_project_picker_reject_stale_results(cx: &mut TestAppContext) {
        cx.executor().allow_parking(); // BrowserService uses real Tokio threads and I/O.
        let dir = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("first.txt"), "first\nsecond").unwrap();
        fs::write(dir.path().join("second.txt"), "other").unwrap();
        let root = Arc::new(ProjectRoot::open(dir.path()).unwrap());
        let store = Store::new(config.path().into());
        let location = Location {
            root: root.clone(),
            directory: dir.path().into(),
            slug: None,
            config: RuntimeConfig::resolve(&Default::default(), None).unwrap(),
        };
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1200.), px(800.)),
                    })),
                    ..Default::default()
                },
                cx,
                |w, cx| {
                    cx.new(|cx| {
                        Shell::new(
                            w,
                            cx,
                            store,
                            BrowserService::new().unwrap(),
                            dir.path().into(),
                        )
                    })
                },
            )
            .unwrap()
        });
        cx.update_window(handle, |_, w, cx| {
            shell.update(cx, |this, cx| {
                this.cancel(&Cancel, w, cx);
                let id = OperationId {
                    project: this.generation,
                    operation: this.listing,
                };
                this.apply_directory(
                    id,
                    Directory {
                        location: location.clone(),
                        entries: root.entries(std::path::Path::new(".")).unwrap(),
                        registry: Registry::default(),
                    },
                    w,
                    cx,
                );
                this.apply_directory(
                    OperationId {
                        project: id.project + 1,
                        operation: id.operation,
                    },
                    Directory {
                        location: location.clone(),
                        entries: vec![],
                        registry: Registry::default(),
                    },
                    w,
                    cx,
                );
                assert_eq!(this.files.len(), 2);
            })
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            w.click("filter", cx);
            w.input("first", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(shell.read(cx).files.len(), 1);
            w.click("projects", cx);
            w.press("escape", cx);
            assert!(!shell.read(cx).projects);
            assert!(shell.read(cx).location.is_some()); // switching UI never detached the browser
            w.click(0usize, cx); // real file preview runs in the background
            assert_eq!(shell.read(cx).files.selected(), Some("first.txt"));
            w.click("copy-selection", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("first.txt")
            );
            w.click("hosts", cx);
            assert!(shell.read(cx).hosts.is_some());
            w.press("escape", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            assert!(shell.read(cx).hosts.is_none());
            assert_eq!(shell.read(cx).files.selected(), Some("first.txt"));
            assert!(shell.read(cx).browser_focus.is_focused(w));
        })
        .unwrap();
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    use gpui_kit::test::{TestAppContextExt, TestWindowExt};
    use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, point, size};
    use std::{fs, time::Duration};

    #[gpui_kit::test]
    async fn real_nested_navigation_buttons_history_and_input_focus(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        for registered in [false, true] {
            let parent = tempfile::tempdir().unwrap();
            let root = parent.path().join("root");
            let child = root.join("child");
            let nested = child.join("nested");
            fs::create_dir_all(&nested).unwrap();
            let config = tempfile::tempdir().unwrap();
            let store = Store::new(config.path().into());
            if registered {
                store.register("Project", root.clone()).unwrap();
            }
            let (handle, shell) = cx.update(|cx| {
                gpui_kit::init(cx);
                bind_keys(cx);
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(0.), px(0.)),
                            size: size(px(1100.), px(720.)),
                        })),
                        ..Default::default()
                    },
                    cx,
                    |w, cx| {
                        cx.new(|cx| {
                            Shell::new(w, cx, store, BrowserService::new().unwrap(), root.clone())
                        })
                    },
                )
                .unwrap()
            });
            cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                shell.read(cx).location.is_some()
            })
            .await;
            for (control, expected) in [
                ("row", child.clone()),
                ("row", nested.clone()),
                ("up", child.clone()),
                ("back", nested.clone()),
                ("alt-left", child.clone()),
                ("alt-right", nested.clone()),
                ("left", child.clone()),
                ("backspace", root.clone()),
            ] {
                cx.update_window(handle, |_, w, cx| {
                    w.render_frame(cx);
                    match control {
                        "row" => w.click(0usize, cx),
                        "up" | "back" => w.click(control, cx),
                        key => w.press(key, cx),
                    }
                })
                .unwrap();
                cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                    let shell = shell.read(cx);
                    shell.listing_cancel.is_none()
                        && shell
                            .location
                            .as_ref()
                            .is_some_and(|l| l.directory == expected)
                })
                .await;
            }
            cx.update_window(handle, |_, w, cx| {
                w.click("filter", cx);
                w.input("hjl", cx);
                assert_eq!(shell.read(cx).filter.read(cx).value().as_str(), "hjl");
                assert_eq!(shell.read(cx).location.as_ref().unwrap().directory, root);
                w.click("up", cx);
            })
            .unwrap();
            let expected = if registered {
                root.clone()
            } else {
                parent.path().into()
            };
            cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                let shell = shell.read(cx);
                shell.listing_cancel.is_none()
                    && shell
                        .location
                        .as_ref()
                        .is_some_and(|l| l.directory == expected)
            })
            .await;
            assert_eq!(
                shell.read_with(cx, |s, _| s
                    .location
                    .as_ref()
                    .unwrap()
                    .root
                    .base()
                    .to_path_buf()),
                expected
            );
            if !registered {
                // Registration belongs to the displayed folder, even when a
                // wider unregistered capability root was opened by Up.
                cx.update_window(handle, |_, w, cx| {
                    w.click("back", cx);
                })
                .unwrap();
                cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                    let s = shell.read(cx);
                    s.listing_cancel.is_none()
                        && s.location.as_ref().is_some_and(|l| l.directory == root)
                })
                .await;
                cx.update_window(handle, |_, w, cx| {
                    w.click(0usize, cx);
                })
                .unwrap();
                cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                    let s = shell.read(cx);
                    s.listing_cancel.is_none()
                        && s.location.as_ref().is_some_and(|l| l.directory == child)
                })
                .await;
                cx.update_window(handle, |_, w, cx| {
                    w.click("projects", cx);
                    w.click("project-name", cx);
                    w.input("Nested project", cx);
                    w.click("register", cx);
                })
                .unwrap();
                cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
                    shell
                        .read(cx)
                        .location
                        .as_ref()
                        .is_some_and(|l| l.slug.is_some())
                })
                .await;
                let registry = Store::new(config.path().into()).registry().unwrap();
                assert_eq!(registry.projects[0].path, child);
            }
        }
    }

    #[gpui_kit::test]
    async fn vanished_directory_keeps_the_previous_location_and_history(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let root = tempfile::tempdir().unwrap();
        let vanished = root.path().join("vanished");
        fs::create_dir(&vanished).unwrap();
        let config = tempfile::tempdir().unwrap();
        let (handle, shell) = cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    Shell::new(
                        w,
                        cx,
                        Store::new(config.path().into()),
                        BrowserService::new().unwrap(),
                        root.path().into(),
                    )
                })
            })
            .unwrap()
        });
        cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
            shell.read(cx).location.is_some()
        })
        .await;
        fs::remove_dir(vanished).unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            w.click(0usize, cx);
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(60), |_, cx| {
            shell.read(cx).listing_cancel.is_none()
        })
        .await;
        shell.read_with(cx, |s, _| {
            assert_eq!(s.location.as_ref().unwrap().directory, root.path());
            assert!(s.history.previous().is_none());
            assert!(!s.status.ends_with("entries"));
        });
    }
}
