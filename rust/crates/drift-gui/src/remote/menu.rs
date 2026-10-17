use super::*;
use crate::browser_menu::{MenuAction, MenuEntry};
use crate::toolbar::ToolbarEvent;
use gpui_kit::EntityId;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    project: u64,
    connection: u64,
    operation: u64,
    revision: u64,
    path: String,
    filter: String,
    loading: bool,
    target: Option<String>,
    show_hidden: bool,
    show_ignored: bool,
    comparison_ready: bool,
    expanded: bool,
}

impl RemotePane {
    pub fn set_comparison_ready(
        &mut self,
        ready: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.comparison_ready != ready {
            self.dismiss_context_menu(window, cx);
            self.comparison_ready = ready;
            cx.notify();
        }
    }

    pub fn dismiss_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, menu)) = self.menu.take() {
            if menu.focus.contains_focused(window, cx) {
                self.focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn menu_valid(&self, snapshot: &Snapshot, cx: &App) -> bool {
        snapshot.project == self.project
            && snapshot.connection == self.connection
            && snapshot.operation == self.operation
            && snapshot.revision == self.menu_revision
            && snapshot.path == self.path
            && snapshot.filter == self.filter.read(cx).value().as_str()
            && snapshot.loading == self.is_loading()
            && snapshot.show_hidden == self.show_hidden
            && snapshot.show_ignored == self.show_ignored
            && snapshot.comparison_ready == self.comparison_ready
            && snapshot.expanded
                == snapshot
                    .target
                    .as_ref()
                    .is_some_and(|path| self.tree.node(path).is_some_and(|node| node.expanded))
            && self
                .session
                .as_ref()
                .is_none_or(|session| session.state() == ConnectionState::Connected)
            && snapshot.target.as_ref().is_none_or(|path| {
                (0..self.files.len()).any(|row| self.files.row(row) == Some(path.as_str()))
                    && self.entries.iter().any(|entry| entry.path == *path)
                    && self
                        .session
                        .as_ref()
                        .is_some_and(|session| session.contains(path))
            })
    }

    fn menu_enabled(&self, snapshot: &Snapshot, action: MenuAction) -> bool {
        let entry = snapshot
            .target
            .as_ref()
            .and_then(|path| self.entries.iter().find(|entry| entry.path == *path));
        let idle = self.has_session() && !self.is_loading();
        let compare = idle && self.comparison_ready;
        match action {
            MenuAction::Open => idle && entry.is_some_and(|e| e.directory || e.regular),
            MenuAction::ToggleTree => idle && entry.is_some_and(|e| e.directory),
            MenuAction::Mark => idle && entry.is_some_and(|e| self.files.can_mark(&e.path)),
            MenuAction::MarkSiblings => idle && entry.is_some(),
            MenuAction::InvertMarks => idle && !self.files.is_empty(),
            MenuAction::ClearMarks => {
                !self.is_loading() && (self.files.range_active() || !self.files.marked().is_empty())
            }
            MenuAction::ComparePath => {
                compare && entry.is_some_and(|e| self.files.can_mark(&e.path))
            }
            MenuAction::CompareMarked => compare,
            MenuAction::CompareProject => compare,
            MenuAction::CopyPath => self.has_session() && !self.path.is_empty(),
            MenuAction::Back => idle && self.history.previous().is_some(),
            MenuAction::Forward => idle && self.history.next().is_some(),
            MenuAction::Up => idle && self.session.as_ref().is_some_and(|s| s.root != self.path),
            MenuAction::Refresh => idle,
            MenuAction::Hidden => !self.is_loading(),
            MenuAction::Cancel => self.is_loading(),
            MenuAction::Disconnect => self.has_session() || self.is_loading(),
            MenuAction::Ignored => {
                idle && self.selection_location.is_some() && self.visibility.is_some()
            }
            MenuAction::Find => false,
        }
    }

    fn menu_entries(&self, snapshot: &Snapshot) -> Vec<Option<MenuEntry>> {
        let item = |action, label| {
            Some(MenuEntry::new(
                action,
                label,
                self.menu_enabled(snapshot, action),
            ))
        };
        let mut entries = Vec::new();
        if let Some(path) = &snapshot.target {
            let directory = self.entries.iter().any(|e| e.path == *path && e.directory);
            entries.push(item(
                MenuAction::Open,
                if directory { "Open folder" } else { "Preview" },
            ));
            if directory {
                entries.push(item(
                    MenuAction::ToggleTree,
                    if self.tree.node(path).is_some_and(|n| n.expanded) {
                        "Collapse folder"
                    } else {
                        "Expand folder"
                    },
                ));
            }
            entries.push(None);
            entries.push(item(
                MenuAction::Mark,
                if self.files.is_marked(path) {
                    "Unmark"
                } else {
                    "Mark"
                },
            ));
            entries.push(item(MenuAction::MarkSiblings, "Mark siblings"));
        }
        entries.extend([
            item(MenuAction::InvertMarks, "Invert visible marks"),
            item(MenuAction::ClearMarks, "Clear all marks"),
            None,
        ]);
        if let Some(path) = &snapshot.target {
            entries.push(item(
                MenuAction::ComparePath,
                if self.entries.iter().any(|e| e.path == *path && e.directory) {
                    "Compare this folder"
                } else {
                    "Compare this file"
                },
            ));
        }
        entries.extend([
            item(MenuAction::CompareMarked, "Compare marked files"),
            item(MenuAction::CompareProject, "Compare project"),
            None,
            item(MenuAction::CopyPath, "Copy path"),
            item(MenuAction::Back, "Back"),
            item(MenuAction::Forward, "Forward"),
            item(MenuAction::Up, "Up"),
            item(MenuAction::Refresh, "Refresh"),
            Some(
                MenuEntry::new(
                    MenuAction::Hidden,
                    "Show hidden",
                    self.menu_enabled(snapshot, MenuAction::Hidden),
                )
                .checked(self.show_hidden),
            ),
            Some(
                MenuEntry::new(
                    MenuAction::Ignored,
                    "Show ignored",
                    self.menu_enabled(snapshot, MenuAction::Ignored),
                )
                .checked(self.show_ignored),
            ),
            None,
            item(MenuAction::Cancel, "Cancel loading"),
            item(MenuAction::Disconnect, "Disconnect"),
        ]);
        entries
    }

    pub(super) fn open_context_menu(
        &mut self,
        _: &OpenContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.open_menu(
            self.files.selected().map(str::to_owned),
            self.menu_anchor,
            window,
            cx,
        );
    }

    pub(super) fn open_menu(
        &mut self,
        target: Option<String>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.has_active_drag() {
            return;
        }
        let snapshot = Snapshot {
            project: self.project,
            connection: self.connection,
            operation: self.operation,
            revision: self.menu_revision,
            path: self.path.clone(),
            filter: self.filter.read(cx).value().to_string(),
            loading: self.is_loading(),
            expanded: target
                .as_ref()
                .is_some_and(|path| self.tree.node(path).is_some_and(|node| node.expanded)),
            target,
            show_hidden: self.show_hidden,
            show_ignored: self.show_ignored,
            comparison_ready: self.comparison_ready,
        };
        if !self.menu_valid(&snapshot, cx) {
            self.dismiss_context_menu(window, cx);
            return;
        }
        self.dismiss_context_menu(window, cx);
        if let Some(path) = &snapshot.target {
            // Moving only the cursor keeps the active range, marks and preview intact.
            self.files.select_path(path);
        }
        self.focus.focus(window, cx);
        let entity = cx.weak_entity();
        let dismiss_entity = entity.clone();
        let callback_snapshot = snapshot.clone();
        let popup_id = Rc::new(Cell::new(None));
        let callback_id = popup_id.clone();
        let menu = BrowserMenu::open(
            self.menu_entries(&snapshot),
            self.focus.clone(),
            position,
            window,
            cx,
            move |action, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.menu_action(&callback_snapshot, callback_id.get(), action, window, cx);
                });
            },
            move |id, window, cx| {
                let _ = dismiss_entity.update(cx, |this, cx| {
                    if this
                        .menu
                        .as_ref()
                        .is_some_and(|(_, menu)| menu.popup.entity_id() == id)
                    {
                        this.dismiss_context_menu(window, cx);
                    }
                });
            },
        );
        popup_id.set(Some(menu.popup.entity_id()));
        self.menu = Some((snapshot, menu));
        cx.notify();
    }

    pub(super) fn menu_action(
        &mut self,
        snapshot: &Snapshot,
        popup_id: Option<EntityId>,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.menu.as_ref().is_some_and(|(current, menu)| {
            current == snapshot && Some(menu.popup.entity_id()) == popup_id
        }) {
            return;
        }
        let valid = self.menu_valid(snapshot, cx) && self.menu_enabled(snapshot, action);
        self.dismiss_context_menu(window, cx);
        if !valid {
            return;
        }
        if let Some(path) = &snapshot.target {
            self.files.select_path(path);
        }
        self.focus.focus(window, cx);
        match action {
            MenuAction::Open => {
                if let Some(row) = self.files.selected_row() {
                    self.choose(row, window, cx);
                }
            }
            MenuAction::ToggleTree => {
                if let Some(path) = &snapshot.target {
                    self.toggle_tree(path.clone(), window, cx);
                }
            }
            MenuAction::Mark => self.files.toggle_mark(),
            MenuAction::MarkSiblings => self.files.mark_siblings(),
            MenuAction::InvertMarks => self.files.invert_visible(),
            MenuAction::ClearMarks => {
                if self.files.range_active() {
                    self.files.clear_marks();
                }
                self.files.clear_marks();
            }
            MenuAction::ComparePath => {
                if let Some(path) = &snapshot.target {
                    cx.emit(RemoteEvent::Compare {
                        project: snapshot.project,
                        connection: snapshot.connection,
                        path: path.clone(),
                    });
                }
            }
            MenuAction::CompareMarked | MenuAction::CompareProject => {
                cx.emit(RemoteEvent::Toolbar {
                    project: snapshot.project,
                    connection: snapshot.connection,
                    event: if action == MenuAction::CompareMarked {
                        ToolbarEvent::CompareMarked
                    } else {
                        ToolbarEvent::CompareProject
                    },
                });
            }
            MenuAction::CopyPath => {
                if snapshot.target.is_some() {
                    self.copy(&CopySelection, window, cx);
                } else {
                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                        snapshot.path.clone(),
                    ));
                }
            }
            MenuAction::Back => self.back(&GoBack, window, cx),
            MenuAction::Forward => self.forward(&GoForward, window, cx),
            MenuAction::Up => self.up(&GoUp, window, cx),
            MenuAction::Refresh => self.refresh(&Refresh, window, cx),
            MenuAction::Hidden => {
                self.show_hidden = !self.show_hidden;
                self.set_entries(cx);
            }
            MenuAction::Cancel => self.cancel(cx),
            MenuAction::Disconnect => {
                self.disconnect(cx);
                self.status("Disconnected".into(), cx);
            }
            MenuAction::Ignored => self.toggle_ignored(window, cx),
            MenuAction::Find => {}
        }
        cx.notify();
    }
}
