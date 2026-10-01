use drift_app::{
    FileList,
    browser::{BrowserService, Directory, Location, OperationId},
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
    App, AppContext, ClipboardItem, Context, Entity, Focusable, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
    div, px, uniform_list,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

gpui_kit::actions!(drift, [FocusFilter, CopySelection, Refresh, Cancel]);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-f", FocusFilter, Some("Drift")),
        KeyBinding::new("cmd-f", FocusFilter, Some("Drift")),
        KeyBinding::new("ctrl-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("cmd-shift-c", CopySelection, Some("Drift")),
        KeyBinding::new("f5", Refresh, Some("Drift")),
        KeyBinding::new("escape", Cancel, Some("Drift")),
    ]);
}
pub struct Shell {
    filter: Entity<InputState>,
    preview: Entity<EditorState>,
    name: Entity<InputState>,
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
    _subscription: Subscription,
}
impl Drop for Shell {
    fn drop(&mut self) {
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
        let subscription = cx.subscribe_in(&filter, window, |this, state, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.files.filter(&state.read(cx).value());
                this.clear_preview(window, cx);
                this.status = format!("{} entries", this.files.len());
                cx.notify();
            }
        });
        filter.focus_handle(cx).focus(window, cx);
        let mut shell = Self {
            filter,
            preview,
            name,
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
            _subscription: subscription,
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
    fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.generation += 1;
        self.listing += 1;
        self.clear_preview(window, cx);
        if let Some(cancel) = self.listing_cancel.take() {
            cancel.cancel();
        }
        self.location = None;
        self.files = FileList::new(vec![]);
        self.entries.clear();
        self.finder = false;
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
        self.status = format!("{} entries", self.files.len());
    }
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.location.clone() else {
            self.open(self.start.clone(), window, cx);
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
        let operation = self.service.read_directory(
            self.store.clone(),
            location,
            id,
            self.show_hidden,
            self.show_ignored,
        );
        self.listing_cancel = Some(operation.cancel);
        self.finder = false;
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
            let mut target = location;
            target.directory = target.root.base().join(&path);
            self.location = Some(target);
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.reload(window, cx);
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
        if let Some(location) = self.location.as_mut()
            && location.directory != location.root.base()
            && let Some(parent) = location.directory.parent()
        {
            location.directory = parent.into();
            self.reload(window, cx);
        }
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
        let operation =
            self.service
                .register(self.store.clone(), name, location.root.base().into(), id);
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
                    .gap_2()
                    .p_3()
                    .border_b_1()
                    .border_color(border)
                    .child(div().font_weight(gpui_kit::FontWeight::BOLD).child("drift"))
                    .child(
                        Button::new("projects")
                            .label("Projects")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.projects = !this.projects;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("up")
                            .label("Up")
                            .disabled(
                                self.location
                                    .as_ref()
                                    .is_none_or(|l| l.directory == l.root.base()),
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
                view.child(
                    div()
                        .flex()
                        .flex_col()
                        .p_3()
                        .gap_2()
                        .border_b_1()
                        .border_color(border)
                        .children(self.registry.active().into_iter().map(|p| {
                            let path = p.path.clone();
                            Button::new(p.slug.clone())
                                .label(p.name.clone())
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.projects = false;
                                    this.open(path.clone(), w, cx);
                                }))
                                .into_any_element()
                        }))
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
                                                let label = format!(
                                                    "{}{}",
                                                    name,
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
            assert_eq!(shell.read(cx).files.selected(), Some("./first.txt"));
            w.click("copy-selection", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("./first.txt")
            );
        })
        .unwrap();
    }
}
