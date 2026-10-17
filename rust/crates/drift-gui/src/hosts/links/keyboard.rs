use super::*;
use crate::actions::{CursorDown, CursorFirst, CursorLast, CursorUp};

gpui_kit::actions!(
    drift_links,
    [FocusList, Search, Choose, Confirm, DefaultConfirm, Reload]
);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", FocusList, Some("DriftLinkFilter")),
        KeyBinding::new("enter", FocusList, Some("DriftLinkFilter")),
        KeyBinding::new("ctrl-f", Search, Some("DriftLinkPicker")),
        KeyBinding::new("cmd-f", Search, Some("DriftLinkPicker")),
        KeyBinding::new("/", Search, Some("DriftLinkList")),
        KeyBinding::new("up", CursorUp, Some("DriftLinkList")),
        KeyBinding::new("k", CursorUp, Some("DriftLinkList")),
        KeyBinding::new("down", CursorDown, Some("DriftLinkList")),
        KeyBinding::new("j", CursorDown, Some("DriftLinkList")),
        KeyBinding::new("home", CursorFirst, Some("DriftLinkList")),
        KeyBinding::new("g", CursorFirst, Some("DriftLinkList")),
        KeyBinding::new("end", CursorLast, Some("DriftLinkList")),
        KeyBinding::new("shift-g", CursorLast, Some("DriftLinkList")),
        KeyBinding::new("enter", Choose, Some("DriftLinkList")),
        KeyBinding::new("r", Reload, Some("DriftLinkList")),
        KeyBinding::new("r", Reload, Some("DriftLinkConfirm")),
        KeyBinding::new("enter", DefaultConfirm, Some("DriftLinkConfirm")),
        KeyBinding::new("y", Confirm, Some("DriftLinkConfirm")),
    ]);
}
impl LinkPicker {
    pub(super) fn visible_targets(&self, cx: &App) -> Vec<(usize, LinkTarget)> {
        let query = self.query.read(cx).value().to_lowercase();
        self.catalog
            .as_ref()
            .into_iter()
            .flat_map(|catalog| {
                catalog
                    .targets
                    .iter()
                    .enumerate()
                    .filter(|(_, target)| {
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
                    .map(|(index, target)| (index, target.clone()))
            })
            .collect()
    }
    pub(super) fn cursor_row(&self, cx: &App) -> usize {
        self.visible_targets(cx)
            .iter()
            .position(|(_, target)| {
                self.cursor.as_ref().is_some_and(|(project, name)| {
                    project == &target.project && name == &target.host.name
                })
            })
            .unwrap_or(0)
    }
    pub(super) fn select_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() || self.selected.is_some() {
            return;
        }
        let targets = self.visible_targets(cx);
        let row = row.min(targets.len().saturating_sub(1));
        self.cursor = targets
            .get(row)
            .map(|(_, target)| (target.project.clone(), target.host.name.clone()));
        self.list_focus.focus(window, cx);
        self.scroll.scroll_to_item(row);
        cx.notify();
    }
    pub(super) fn choose_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() || self.selected.is_some() {
            return;
        }
        if let Some((_, target)) = self.visible_targets(cx).get(self.cursor_row(cx)) {
            self.choose(target.clone(), window, cx);
        }
    }
    pub(super) fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        if let Some(target) = self.selected.clone() {
            self.request(
                HostCommand::SelectLink {
                    expected: Box::new(target),
                },
                window,
                cx,
            );
        }
    }
}
