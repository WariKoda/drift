use super::*;

impl Shell {
    pub(super) fn pane_view(&self, side: PaneSide) -> PaneView {
        match side {
            PaneSide::Local => self.local_view,
            PaneSide::Remote => self.remote_view,
        }
    }

    pub(super) fn preview_pane(&self, side: PaneSide) -> Entity<PreviewPane> {
        match side {
            PaneSide::Local => self.local_preview.clone(),
            PaneSide::Remote => self.remote_preview.clone(),
        }
    }

    pub(super) fn focused_side(&self, window: &Window, cx: &gpui_kit::App) -> PaneSide {
        if self.remote_focus.contains_focused(window, cx) {
            PaneSide::Remote
        } else if self.local_focus.contains_focused(window, cx) {
            PaneSide::Local
        } else {
            self.active_pane
        }
    }

    pub(super) fn set_pane_view(
        &mut self,
        side: PaneSide,
        view: PaneView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        let project = self.browser.read(cx).id().project;
        if view == PaneView::Preview {
            let preview = self.preview_pane(side);
            let selected = match side {
                PaneSide::Local => self.browser.read(cx).selected().map(PathBuf::from),
                PaneSide::Remote => self.remote.read(cx).selected().map(PathBuf::from),
            };
            if selected.as_ref().is_some_and(|path| {
                preview
                    .read(cx)
                    .requested_for(project, path, side == PaneSide::Remote)
            }) && preview.read(cx).can_show(project)
            {
                self.reveal_preview(side, window, cx);
            } else {
                let can_preview = match side {
                    PaneSide::Local => self.browser.read(cx).can_preview(),
                    PaneSide::Remote => self.remote.read(cx).can_preview(),
                };
                if can_preview {
                    match side {
                        PaneSide::Local => self
                            .browser
                            .update(cx, |pane, cx| pane.preview_selected(window, cx)),
                        PaneSide::Remote => self
                            .remote
                            .update(cx, |pane, cx| pane.preview_selected(window, cx)),
                    }
                } else if preview.read(cx).can_show(project) {
                    self.reveal_preview(side, window, cx);
                }
            }
            return;
        }
        if self.pane_view(side) != view {
            self.navigation_epoch += 1;
        }
        self.active_pane = side;
        match side {
            PaneSide::Local => {
                self.local_view = PaneView::Files;
                self.browser.update(cx, |pane, cx| {
                    pane.set_navigation_active(true, cx);
                    pane.focus(window, cx);
                });
            }
            PaneSide::Remote => {
                self.remote_view = PaneView::Files;
                self.remote
                    .update(cx, |pane, cx| pane.set_workspace_active(true, true, cx));
                self.remote.focus_handle(cx).focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn reveal_preview(
        &mut self,
        side: PaneSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        if self.pane_view(side) != PaneView::Preview {
            self.navigation_epoch += 1;
        }
        self.active_pane = side;
        match side {
            PaneSide::Local => {
                self.local_view = PaneView::Preview;
                self.browser
                    .update(cx, |pane, cx| pane.set_navigation_active(false, cx));
            }
            PaneSide::Remote => {
                self.remote_view = PaneView::Preview;
                self.remote
                    .update(cx, |pane, cx| pane.set_workspace_active(true, false, cx));
            }
        }
        self.preview_pane(side)
            .update(cx, |pane, cx| pane.focus(window, cx));
        cx.notify();
    }

    pub(super) fn clear_local_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.browser_screen_active(cx)
            && self.local_view == PaneView::Preview
            && self.local_focus.contains_focused(window, cx);
        self.local_view = PaneView::Files;
        self.local_preview.update(cx, |pane, cx| {
            pane.select(None, self.browser.read(cx).id().project, window, cx)
        });
        if restore {
            self.active_pane = PaneSide::Local;
            self.browser.update(cx, |pane, cx| pane.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn clear_remote_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.browser_screen_active(cx)
            && self.remote_view == PaneView::Preview
            && self.remote_focus.contains_focused(window, cx);
        self.remote_view = PaneView::Files;
        self.remote_preview_session = None;
        self.remote_preview.update(cx, |pane, cx| {
            pane.select(None, self.browser.read(cx).id().project, window, cx)
        });
        if restore {
            self.active_pane = PaneSide::Remote;
            self.remote.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn refresh_side(
        &mut self,
        side: PaneSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.browser_screen_active(cx) || self.split.is_resizing() {
            return;
        }
        self.set_pane_view(side, PaneView::Files, window, cx);
        let project = self.browser.read(cx).id().project;
        self.preview_pane(side)
            .update(cx, |pane, cx| pane.select(None, project, window, cx));
        match side {
            PaneSide::Local => self
                .browser
                .update(cx, |pane, cx| pane.refresh(&Refresh, window, cx)),
            PaneSide::Remote => {
                self.remote_preview_session = None;
                self.remote
                    .update(cx, |pane, cx| pane.refresh(&Refresh, window, cx));
            }
        }
    }

    pub(super) fn cancel_side(
        &mut self,
        side: PaneSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = self.focused_side(window, cx) == side;
        match side {
            PaneSide::Local => {
                self.cancel_registration(cx);
                self.browser.update(cx, |pane, cx| {
                    pane.command(BrowserCommand::Cancel, window, cx)
                });
                self.clear_local_preview(window, cx);
                if focused {
                    self.browser.update(cx, |pane, cx| pane.focus(window, cx));
                }
            }
            PaneSide::Remote => {
                self.remote.update(cx, |pane, cx| pane.cancel(cx));
                self.clear_remote_preview(window, cx);
                if focused {
                    self.remote.focus_handle(cx).focus(window, cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn preview_event(
        &mut self,
        side: PaneSide,
        event: &PreviewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preview = self.preview_pane(side);
        let current = preview.read(cx).id();
        let project = self.browser.read(cx).id().project;
        match event {
            PreviewEvent::Loaded(id) if *id == current && id.project == project => {
                self.status = match side {
                    PaneSide::Local => "Local preview loaded".into(),
                    PaneSide::Remote => "Remote preview loaded".into(),
                };
            }
            PreviewEvent::Failed(id, error) if *id == current && id.project == project => {
                self.status.clone_from(error)
            }
            PreviewEvent::TextCopied(id) if *id == current && id.project == project => {
                self.status = "Preview text copied".into()
            }
            PreviewEvent::Copied(path)
                if current.project == project && self.browser_screen_active(cx) =>
            {
                self.status = format!("Copied {path}")
            }
            PreviewEvent::Close(id)
                if *id == current
                    && id.project == project
                    && self.browser_screen_active(cx)
                    && self.pane_view(side) == PaneView::Preview =>
            {
                self.close_preview(side, window, cx);
            }
            _ => return,
        }
        cx.notify();
    }
}
