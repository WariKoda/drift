use super::*;
use crate::browser_menu::{BrowserMenu, MenuAction, MenuEntry};
use crate::toolbar::ToolbarEvent;
use gpui_kit::{Pixels, Point};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct MenuSnapshot {
    serial: u64,
    revision: u64,
    id: OperationId,
    directory: Option<PathBuf>,
    filter: String,
    loading: bool,
    hidden: bool,
    ignored: bool,
    finder: bool,
    connection: Option<OperationId>,
    target: Option<String>,
    expanded: bool,
}

impl BrowserPane {
    pub fn set_comparison_context(
        &mut self,
        connection: Option<OperationId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.comparison_connection != connection {
            self.dismiss_context_menu(window, cx);
            self.comparison_connection = connection;
        }
    }
    pub fn dismiss_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, menu)) = self.menu.take() {
            if menu.focus.contains_focused(window, cx) {
                self.focus(window, cx);
            }
            cx.notify();
        }
    }
    fn menu_snapshot(&self, target: Option<String>, cx: &App) -> MenuSnapshot {
        MenuSnapshot {
            serial: self.menu_serial,
            revision: self.menu_revision,
            id: self.id(),
            directory: self.location.as_ref().map(|l| l.directory.clone()),
            filter: self.filter.read(cx).value().to_string(),
            loading: self.is_loading(),
            hidden: self.show_hidden,
            ignored: self.show_ignored,
            finder: self.finder,
            connection: self.comparison_connection,
            expanded: target
                .as_ref()
                .is_some_and(|p| self.tree.node(p).is_some_and(|n| n.expanded)),
            target,
        }
    }
    fn menu_valid(&self, snapshot: &MenuSnapshot, cx: &App) -> bool {
        *snapshot == self.menu_snapshot(snapshot.target.clone(), cx)
            && snapshot.target.as_ref().is_none_or(|p| {
                (0..self.files.len()).any(|row| self.files.row(row) == Some(p.as_str()))
            })
    }
    pub(super) fn validate_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .menu
            .as_ref()
            .is_some_and(|(snapshot, _)| !self.menu_valid(snapshot, cx))
        {
            self.dismiss_context_menu(window, cx);
        }
    }
    pub(super) fn open_context_menu(
        &mut self,
        target: Option<String>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.has_active_drag() {
            return;
        }
        if let Some(path) = &target {
            if !(0..self.files.len()).any(|row| self.files.row(row) == Some(path.as_str())) {
                return;
            }
            self.files.select_path(path);
        }
        self.dismiss_context_menu(window, cx);
        self.focus(window, cx);
        self.menu_serial += 1;
        let snapshot = self.menu_snapshot(target, cx);
        let entries = self.menu_entries(&snapshot);
        let owner = cx.weak_entity();
        let dismiss_owner = owner.clone();
        let action_snapshot = snapshot.clone();
        let menu = BrowserMenu::open(
            entries,
            self.browser_focus.clone(),
            position,
            window,
            cx,
            move |action, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.menu_action(action, &action_snapshot, window, cx)
                });
            },
            move |id, _, cx| {
                let _ = dismiss_owner.update(cx, |this, cx| {
                    if this
                        .menu
                        .as_ref()
                        .is_some_and(|(_, menu)| menu.popup.entity_id() == id)
                    {
                        this.menu = None;
                        cx.notify();
                    }
                });
            },
        );
        self.menu = Some((snapshot, menu));
        cx.notify();
    }
    pub(super) fn keyboard_menu(
        &mut self,
        _: &OpenContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.open_context_menu(
            self.files.selected().map(str::to_owned),
            self.menu_anchor,
            window,
            cx,
        );
    }
    fn menu_entries(&self, snapshot: &MenuSnapshot) -> Vec<Option<MenuEntry>> {
        use MenuAction::*;
        let mut entries = vec![];
        let ready = self.location.is_some() && !self.is_loading();
        if let Some(path) = &snapshot.target {
            let directory = self.tree.node(path).is_some_and(|n| n.directory);
            entries.push(Some(MenuEntry::new(
                Open,
                if directory { "Open folder" } else { "Preview" },
                ready,
            )));
            if directory && !self.finder {
                entries.push(Some(MenuEntry::new(
                    ToggleTree,
                    if snapshot.expanded {
                        "Collapse folder"
                    } else {
                        "Expand folder"
                    },
                    ready,
                )));
            }
            entries.push(None);
            entries.push(Some(
                MenuEntry::new(
                    Mark,
                    if self.files.is_marked(path) {
                        "Unmark"
                    } else {
                        "Mark"
                    },
                    ready,
                )
                .checked(self.files.is_marked(path)),
            ));
        }
        entries.extend([
            Some(MenuEntry::new(
                MarkSiblings,
                "Mark siblings",
                ready && snapshot.target.is_some(),
            )),
            Some(MenuEntry::new(
                InvertMarks,
                "Invert visible marks",
                ready && !self.files.is_empty(),
            )),
            Some(MenuEntry::new(
                ClearMarks,
                "Clear all marks",
                !self.files.marked().is_empty() || self.files.range_active(),
            )),
            None,
        ]);
        if snapshot.target.is_some() {
            entries.push(Some(MenuEntry::new(
                ComparePath,
                "Compare this file/folder",
                ready && snapshot.connection.is_some(),
            )));
        }
        entries.extend([
            Some(MenuEntry::new(
                CompareMarked,
                "Compare marked files",
                ready && snapshot.connection.is_some(),
            )),
            Some(MenuEntry::new(
                CompareProject,
                "Compare project",
                ready && snapshot.connection.is_some(),
            )),
            None,
        ]);
        if snapshot.target.is_some() {
            entries.push(Some(MenuEntry::new(CopyPath, "Copy path", true)));
        }
        entries.extend([
            Some(MenuEntry::new(Back, "Back", ready && self.can_back())),
            Some(MenuEntry::new(
                Forward,
                "Forward",
                ready && self.can_forward(),
            )),
            Some(MenuEntry::new(Up, "Up", ready && self.can_up())),
            Some(MenuEntry::new(Refresh, "Refresh", ready)),
            Some(MenuEntry::new(Hidden, "Show hidden", ready).checked(self.show_hidden)),
            Some(MenuEntry::new(Ignored, "Show ignored", ready).checked(self.show_ignored)),
            Some(MenuEntry::new(Find, "Find files", ready)),
            None,
            Some(MenuEntry::new(Cancel, "Cancel loading", self.is_loading())),
        ]);
        entries
    }
    fn menu_action(
        &mut self,
        action: MenuAction,
        snapshot: &MenuSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .menu
            .as_ref()
            .is_none_or(|(current, _)| current != snapshot)
            || !self.menu_valid(snapshot, cx)
            || !self
                .menu_entries(snapshot)
                .iter()
                .flatten()
                .any(|e| e.action == action && e.enabled)
        {
            return;
        }
        self.dismiss_context_menu(window, cx);
        self.focus(window, cx);
        if let Some(path) = &snapshot.target {
            self.files.select_path(path);
        }
        use MenuAction::*;
        match action {
            Open => self.choose(self.files.selected_row().unwrap_or(0), window, cx),
            ToggleTree => {
                if let Some(path) = &snapshot.target {
                    self.toggle_tree(path.clone(), window, cx);
                }
            }
            Mark => {
                self.files.toggle_mark();
            }
            MarkSiblings => self.files.mark_siblings(),
            InvertMarks => self.files.invert_visible(),
            ClearMarks => {
                if self.files.range_active() {
                    self.files.clear_marks();
                }
                self.files.clear_marks();
            }
            ComparePath => {
                if let (Some(path), Some(connection)) = (&snapshot.target, snapshot.connection) {
                    cx.emit(BrowserEvent::Compare {
                        id: snapshot.id,
                        connection,
                        path: PathBuf::from(path),
                    });
                }
            }
            CompareMarked | CompareProject => {
                if let Some(connection) = snapshot.connection {
                    cx.emit(BrowserEvent::Toolbar {
                        id: snapshot.id,
                        connection,
                        event: if action == CompareMarked {
                            ToolbarEvent::CompareMarked
                        } else {
                            ToolbarEvent::CompareProject
                        },
                    });
                }
            }
            CopyPath => self.copy_selection(&CopySelection, window, cx),
            Back => self.command(BrowserCommand::Back, window, cx),
            Forward => self.command(BrowserCommand::Forward, window, cx),
            Up => self.command(BrowserCommand::Up, window, cx),
            Refresh => self.command(BrowserCommand::Refresh, window, cx),
            Hidden => self.command(BrowserCommand::Hidden, window, cx),
            Ignored => self.command(BrowserCommand::Ignored, window, cx),
            Find => self.command(BrowserCommand::Find, window, cx),
            Cancel => self.command(BrowserCommand::Cancel, window, cx),
            Disconnect => unreachable!("not offered in the local menu"),
        }
        cx.notify();
    }
}
