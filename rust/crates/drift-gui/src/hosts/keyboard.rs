use super::*;
use crate::actions::{CursorDown, CursorFirst, CursorLast, CursorUp};

gpui_kit::actions!(
    drift_hosts,
    [
        FocusList,
        New,
        Edit,
        Duplicate,
        Remove,
        Test,
        ResetTrust,
        Link,
        Reload,
        Save,
        Confirm,
        DefaultConfirm,
        Scope
    ]
);
pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", FocusList, Some("DriftHostFilter")),
        KeyBinding::new("enter", FocusList, Some("DriftHostFilter")),
        KeyBinding::new("/", Search, Some("DriftHostList")),
        KeyBinding::new("up", CursorUp, Some("DriftHostList")),
        KeyBinding::new("k", CursorUp, Some("DriftHostList")),
        KeyBinding::new("down", CursorDown, Some("DriftHostList")),
        KeyBinding::new("j", CursorDown, Some("DriftHostList")),
        KeyBinding::new("home", CursorFirst, Some("DriftHostList")),
        KeyBinding::new("g", CursorFirst, Some("DriftHostList")),
        KeyBinding::new("end", CursorLast, Some("DriftHostList")),
        KeyBinding::new("shift-g", CursorLast, Some("DriftHostList")),
        KeyBinding::new("n", New, Some("DriftHostList")),
        KeyBinding::new("e", Edit, Some("DriftHostList")),
        KeyBinding::new("enter", Edit, Some("DriftHostList")),
        KeyBinding::new("c", Duplicate, Some("DriftHostList")),
        KeyBinding::new("d", Remove, Some("DriftHostList")),
        KeyBinding::new("delete", Remove, Some("DriftHostList")),
        KeyBinding::new("t", Test, Some("DriftHostList")),
        KeyBinding::new("r", ResetTrust, Some("DriftHostList")),
        KeyBinding::new("l", Link, Some("DriftHostList")),
        KeyBinding::new("f5", Reload, Some("DriftHostList")),
        KeyBinding::new("tab", Scope, Some("DriftHostList")),
        KeyBinding::new("shift-tab", Scope, Some("DriftHostList")),
        KeyBinding::new("ctrl-s", Save, Some("DriftHostForm")),
        KeyBinding::new("cmd-s", Save, Some("DriftHostForm")),
        KeyBinding::new("enter", DefaultConfirm, Some("DriftHostConfirm")),
        KeyBinding::new("y", Confirm, Some("DriftHostConfirm")),
    ]);
}
impl HostManager {
    pub(super) fn list_active(&self) -> bool {
        self.cancel.is_none()
            && self.form.is_none()
            && self.delete.is_none()
            && self.tools.is_none()
            && self.links.is_none()
    }
    pub(super) fn visible_hosts(&self, cx: &App) -> Vec<Host> {
        let query = self.query.read(cx).value().to_lowercase();
        self.catalog
            .as_ref()
            .into_iter()
            .flat_map(|c| &c.hosts)
            .filter(|h| {
                h.name.to_lowercase().contains(&query) || h.hostname.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }
    pub(super) fn cursor_row(&self, cx: &App) -> usize {
        self.visible_hosts(cx)
            .iter()
            .position(|h| Some(&h.name) == self.cursor.as_ref())
            .unwrap_or(0)
    }
    pub(super) fn selected_host(&self, cx: &App) -> Option<Host> {
        self.visible_hosts(cx).get(self.cursor_row(cx)).cloned()
    }
    pub(super) fn select_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.list_active() {
            return;
        }
        let hosts = self.visible_hosts(cx);
        let row = row.min(hosts.len().saturating_sub(1));
        self.cursor = hosts.get(row).map(|h| h.name.clone());
        self.list_focus.focus(window, cx);
        self.scroll.scroll_to_item(row);
        cx.notify();
    }
    pub(super) fn ask_delete(&mut self, host: Host, window: &mut Window, cx: &mut Context<Self>) {
        if !self.list_active() {
            return;
        }
        self.cursor = Some(host.name.clone());
        self.delete = Some(host);
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel.is_some()
            || self.form.is_some()
            || self.tools.is_some()
            || self.links.is_some()
        {
            return;
        }
        if let Some(expected) = self.delete.clone() {
            self.request(
                HostCommand::Delete {
                    expected: Box::new(expected),
                },
                window,
                cx,
            );
        }
    }
    pub(super) fn reset_selected_trust(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.list_active() {
            return;
        }
        if let Some(host) = self.selected_host(cx)
            && self.catalog.as_ref().is_some_and(|c| {
                c.runtime
                    .hosts
                    .iter()
                    .any(|h| h.name == host.name && h.protocol == "ftps")
            })
        {
            self.open_tools(host, Mode::Reset, window, cx);
        }
    }
}
