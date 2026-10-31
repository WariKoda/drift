use super::*;
use gpui_kit::component::button::Button;
use gpui_kit::{
    AnyElement, EntityId, FocusHandle, ScrollHandle, StatefulInteractiveElement, point, px,
};

#[derive(PartialEq, Eq)]
struct HelpOwner {
    project: u64,
    finder: bool,
    hosts: Option<(EntityId, u64)>,
    certificate: Option<(u64, u64)>,
    projects: bool,
    settings: bool,
    comparison: bool,
    local_view: PaneView,
    remote_view: PaneView,
}

pub(super) struct ShortcutHelp {
    pub(super) focus: FocusHandle,
    restore: Option<FocusHandle>,
    owner: HelpOwner,
    scroll: ScrollHandle,
}

const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Browser",
        &[
            (
                "↑ / ↓ / k / j · Home / End / g / Shift+G",
                "Move the file cursor",
            ),
            (
                "Enter / → / l · ← / h",
                "Preview a file, expand or collapse a directory",
            ),
            (
                "Tab / Shift+Tab · @",
                "Switch local/remote panes · Show remote targets",
            ),
            (
                "/ or Ctrl/Cmd+F · ↓ / Enter in filter",
                "Filter · Return to results without opening a file",
            ),
            (
                "f · r / F5 · Alt+← / → / ↑",
                "Fuzzy-find project files · Refresh · Back / forward / parent",
            ),
            (
                "Ctrl/Cmd+Alt+F · Finder Return button",
                "Restore the previous local browser view; keep changed marks",
            ),
            (
                "Space · v · Shift+↑ / ↓ · Shift+V · *",
                "Mark · Range · Extend range · Mark siblings · Invert visible marks",
            ),
            (
                "Esc · . · Shift+I",
                "Cancel range, clear filter or marks · Hidden · Ignored local/mapped remote paths",
            ),
            (
                "Ctrl+Esc",
                "Cancel active browser/preview work (also from filters)",
            ),
            (
                "Shift+F10 / Menu · s",
                "Context menu · Compare marked files (never transfers immediately)",
            ),
            ("Shift+P · Shift+H", "Projects · Hosts"),
        ],
    ),
    (
        "Preview and layout",
        &[
            (
                "p · Ctrl/Cmd+Alt+P",
                "Toggle preview in the focused pane · Focus that pane's visible preview",
            ),
            (
                "c · Ctrl/Cmd+Shift+C",
                "Copy the focused pane's visible preview text · Copy its path",
            ),
            (
                "Native arrows / PageUp / PageDown · Ctrl/Cmd+C",
                "Scroll/select in the focused preview · Copy selected text",
            ),
            (
                "Ctrl+Home / End (Linux) · Cmd+↑ / ↓ (macOS)",
                "Preview document boundaries",
            ),
            (
                "Esc in preview",
                "Return that pane to Files; keep its cached preview",
            ),
            ("Ctrl+Alt+← / → / 0", "Resize panes / reset saved sizes"),
            (
                "Esc during resize",
                "End only the resize gesture; file work continues",
            ),
        ],
    ),
    (
        "Comparison and diff",
        &[
            (
                "Tab / Shift+Tab · n / p",
                "Switch list/diff · Next / previous file",
            ),
            (
                "↑ / ↓ / k / j · Home / End / g / Shift+G",
                "Move file cursor or scroll diff",
            ),
            (
                "PageUp / PageDown · Ctrl+U / D · [ / ]",
                "Diff page · Half-page · Previous / next hunk",
            ),
            (
                "Space · u / d · Shift+A",
                "Cycle direction · Prepare upload / download · Cycle all directions",
            ),
            (
                "s / Shift+S · Ctrl/Cmd+Enter",
                "Prepare selected / all sync · Explicitly confirm the displayed plan",
            ),
            (
                "Enter / l · h · c",
                "Expand context · Fold context · Toggle folds",
            ),
            (
                "r / F5 · i · e · Esc",
                "Refresh · Include ignored paths · Errors · Dismiss / cancel / back",
            ),
            ("Ctrl/Cmd+C", "Copy selected diff text"),
        ],
    ),
    (
        "Forms and dialogs",
        &[
            (
                "Tab / Shift+Tab · Enter / Space",
                "Focus native controls · Activate the focused button",
            ),
            (
                "Ctrl/Cmd+F · ↓ / Enter in filter",
                "Filter a list · Return to results",
            ),
            (
                "Ctrl/Cmd+S · Esc",
                "Save form · Cancel/back (does not undo a committed write)",
            ),
            ("↑ / ↓ · Home / End", "Move list cursor"),
            (
                "n · e · d in host/project lists · c in Hosts",
                "New · Edit · Delete prompt · Duplicate host",
            ),
            (
                "y / Enter at a confirmation's default target",
                "Confirm; Enter on Cancel remains Cancel",
            ),
            (
                "F1 · ? in browser/diff · Esc in help",
                "Shortcut help · Return to the invoking screen",
            ),
        ],
    ),
];

impl Shell {
    fn help_owner(&self, cx: &gpui_kit::App) -> HelpOwner {
        HelpOwner {
            project: self.browser.read(cx).id().project,
            finder: self.browser.read(cx).finder_active()
                && self.hosts.is_none()
                && self.settings.is_none()
                && self.certificate.is_none()
                && !self.projects.read(cx).visible()
                && !self.comparison.read(cx).visible(),
            hosts: self
                .hosts
                .as_ref()
                .map(|hosts| (hosts.entity_id(), hosts.read(cx).operation())),
            certificate: self.certificate.as_ref().map(|(p, c, _)| (*p, *c)),
            projects: self.projects.read(cx).visible(),
            settings: self.settings.is_some(),
            comparison: self.comparison.read(cx).visible(),
            local_view: self.local_view,
            remote_view: self.remote_view,
        }
    }

    pub(super) fn modal_restore_focus(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<FocusHandle> {
        // Deactivation retires native host selectors, even when closed. Keep
        // modal restoration on the stable visible pane instead of its trigger.
        if self.remote.read(cx).host_select_focused(window, cx) {
            if self.remote_view == PaneView::Preview {
                self.remote_preview
                    .update(cx, |pane, cx| pane.focus(window, cx));
                window.focused(cx)
            } else {
                Some(self.remote.focus_handle(cx))
            }
        } else {
            window.focused(cx)
        }
    }

    pub(super) fn show_help(&mut self, _: &ShowHelp, window: &mut Window, cx: &mut Context<Self>) {
        if self.help.is_some() {
            self.close_help(window, cx);
            return;
        }
        if self.startup.is_some() || self.folder_prompt || self.split.is_resizing() {
            return;
        }
        let restore = self.modal_restore_focus(window, cx);
        self.deactivate_browser_navigation(cx);
        self.comparison
            .update(cx, |pane, cx| pane.deactivate_layout(window, cx));
        if let Some(hosts) = &self.hosts {
            hosts.update(cx, |hosts, _| hosts.deactivate());
        }
        let focus = cx.focus_handle();
        self.help = Some(ShortcutHelp {
            restore,
            owner: self.help_owner(cx),
            focus: focus.clone(),
            scroll: ScrollHandle::new(),
        });
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(help) = self.help.take() else { return };
        if help.owner == self.help_owner(cx)
            && let Some(focus) = help.restore
        {
            focus.focus(window, cx);
        } else {
            self.focus_workspace(window, cx);
        }
        cx.notify();
    }

    fn scroll_help(&mut self, amount: f32, edge: Option<bool>, cx: &mut Context<Self>) {
        let Some(help) = &self.help else { return };
        let offset = match edge {
            Some(false) => px(0.),
            Some(true) => -help.scroll.max_offset().y,
            None => help.scroll.offset().y - px(amount),
        };
        help.scroll.set_offset(point(px(0.), offset));
        cx.notify();
    }

    pub(super) fn render_help(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = self.help_owner(cx);
        let restore = self.modal_restore_focus(window, cx);
        let help = self.help.as_mut().expect("help is visible");
        if !help.focus.contains_focused(window, cx) {
            if let Some(focus) = restore
                && focus != help.focus
            {
                help.restore = Some(focus);
                help.owner = owner;
            }
            help.focus.focus(window, cx);
        }
        div().id("shortcut-help").test_support().key_context("DriftHelp")
            .track_focus(&help.focus).flex().flex_col().size_full()
            .on_action(cx.listener(|this, _: &Cancel, w, cx| this.close_help(w, cx)))
            .on_action(cx.listener(|this, _: &ScrollUp, _, cx| this.scroll_help(-40., None, cx)))
            .on_action(cx.listener(|this, _: &ScrollDown, _, cx| this.scroll_help(40., None, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                let height = this.help.as_ref().unwrap().scroll.bounds().size.height;
                this.scroll_help(-f32::from(height), None, cx);
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                let height = this.help.as_ref().unwrap().scroll.bounds().size.height;
                this.scroll_help(f32::from(height), None, cx);
            }))
            .on_action(cx.listener(|this, _: &CursorFirst, _, cx| this.scroll_help(0., Some(false), cx)))
            .on_action(cx.listener(|this, _: &CursorLast, _, cx| this.scroll_help(0., Some(true), cx)))
            .child(div().p_3().border_b_1().border_color(cx.theme().border).child("Keyboard shortcuts"))
            .child(div().id("shortcut-help-body").test_support().flex_1().min_h_0().overflow_y_scroll()
                .track_scroll(&help.scroll).p_3().children(SHORTCUTS.iter().map(|(title, entries)| {
                    div().flex().flex_col().gap_2().mb_4().child(*title)
                        .children(entries.iter().map(|(keys, description)| {
                            div().flex().flex_wrap().gap_3().child(*keys).child(*description)
                        }))
                })))
            .child(div().flex().flex_wrap().p_3().gap_2()
                .child("Letters remain text in editable inputs. Transfers always require confirmation.")
                .child(Button::new("shortcut-help-close").label("Close help")
                    .on_click(cx.listener(|this, _, w, cx| this.close_help(w, cx)))))
            .into_any_element()
    }

    pub(super) fn toggle_preview(
        &mut self,
        _: &TogglePreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        let side = self.focused_side(window, cx);
        if self.pane_view(side) == PaneView::Preview {
            self.close_preview(side, window, cx);
            return;
        }
        let can_preview = match side {
            PaneSide::Local => self.browser.read(cx).can_preview(),
            PaneSide::Remote => self.remote.read(cx).can_preview(),
        };
        if can_preview {
            self.set_pane_view(side, PaneView::Preview, window, cx);
        }
    }

    pub(super) fn focus_preview(
        &mut self,
        _: &FocusPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        let side = self.focused_side(window, cx);
        if self.pane_view(side) == PaneView::Preview {
            self.active_pane = side;
            self.preview_pane(side)
                .update(cx, |pane, cx| pane.focus(window, cx));
        }
    }

    pub(super) fn copy_preview(
        &mut self,
        _: &CopyPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        let side = self.focused_side(window, cx);
        if self.pane_view(side) == PaneView::Preview {
            self.preview_pane(side)
                .update(cx, |pane, cx| pane.copy_text(cx));
        }
    }

    pub(super) fn close_preview(
        &mut self,
        side: PaneSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        self.set_pane_view(side, PaneView::Files, window, cx);
    }
}
