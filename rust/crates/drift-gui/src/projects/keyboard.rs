use super::*;
use crate::actions::{CursorDown, CursorFirst, CursorLast, CursorUp};
use gpui_kit::{App, KeyBinding};

gpui_kit::actions!(
    drift_projects,
    [
        FocusList, Search, New, Edit, Open, Remove, Archive, Archived, Reload, Save, Confirm
    ]
);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", FocusList, Some("DriftProjectFilter")),
        KeyBinding::new("enter", FocusList, Some("DriftProjectFilter")),
        KeyBinding::new("ctrl-f", Search, Some("DriftProjects")),
        KeyBinding::new("cmd-f", Search, Some("DriftProjects")),
        KeyBinding::new("/", Search, Some("DriftProjectList")),
        KeyBinding::new("up", CursorUp, Some("DriftProjectList")),
        KeyBinding::new("k", CursorUp, Some("DriftProjectList")),
        KeyBinding::new("down", CursorDown, Some("DriftProjectList")),
        KeyBinding::new("j", CursorDown, Some("DriftProjectList")),
        KeyBinding::new("home", CursorFirst, Some("DriftProjectList")),
        KeyBinding::new("g", CursorFirst, Some("DriftProjectList")),
        KeyBinding::new("end", CursorLast, Some("DriftProjectList")),
        KeyBinding::new("shift-g", CursorLast, Some("DriftProjectList")),
        KeyBinding::new("n", New, Some("DriftProjectList")),
        KeyBinding::new("e", Edit, Some("DriftProjectList")),
        KeyBinding::new("enter", Open, Some("DriftProjectList")),
        KeyBinding::new("d", Remove, Some("DriftProjectList")),
        KeyBinding::new("delete", Remove, Some("DriftProjectList")),
        KeyBinding::new("a", Archive, Some("DriftProjectList")),
        KeyBinding::new(".", Archived, Some("DriftProjectList")),
        KeyBinding::new("r", Reload, Some("DriftProjectList")),
        KeyBinding::new("ctrl-s", Save, Some("DriftProjectForm")),
        KeyBinding::new("cmd-s", Save, Some("DriftProjectForm")),
        KeyBinding::new("enter", Confirm, Some("DriftProjectConfirm")),
        KeyBinding::new("y", Confirm, Some("DriftProjectConfirm")),
    ]);
}
impl ProjectsPanel {
    pub(super) fn visible_projects(&self, cx: &App) -> Vec<Project> {
        let query = self.query.read(cx).value().to_lowercase();
        (if self.show_archived {
            self.registry.all()
        } else {
            self.registry.active()
        })
        .into_iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query)
                || p.slug.to_lowercase().contains(&query)
                || p.path.to_string_lossy().to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
    }
    pub(super) fn cursor_row(&self, cx: &App) -> usize {
        self.visible_projects(cx)
            .iter()
            .position(|p| Some(&p.slug) == self.cursor.as_ref())
            .unwrap_or(0)
    }
    pub(super) fn selected_project(&self, cx: &App) -> Option<Project> {
        self.visible_projects(cx).get(self.cursor_row(cx)).cloned()
    }
    pub(super) fn select_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading() || self.form.is_some() || self.delete.is_some() {
            return;
        }
        let projects = self.visible_projects(cx);
        let row = row.min(projects.len().saturating_sub(1));
        self.cursor = projects.get(row).map(|p| p.slug.clone());
        self.list_focus.focus(window, cx);
        self.scroll.scroll_to_item(row);
        cx.notify();
    }
    pub(super) fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(expected) = self.delete.clone() {
            self.request(
                ProjectCommand::Remove {
                    expected: Box::new(expected),
                },
                window,
                cx,
            );
        }
    }
    pub(super) fn toggle_archived(&mut self, cx: &mut Context<Self>) {
        self.show_archived = !self.show_archived;
        self.scroll.scroll_to_item(self.cursor_row(cx));
        cx.notify();
    }
}
