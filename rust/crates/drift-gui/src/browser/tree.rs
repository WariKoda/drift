use super::*;

impl BrowserPane {
    pub(super) fn rebuild_tree(&mut self, cx: &mut Context<Self>) {
        self.files
            .replace_entries(self.tree.nodes().iter().map(|n| n.path.clone()).collect());
        self.files.filter(&self.filter.read(cx).value());
    }
    pub(super) fn restore_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(path) = self.tree_restore.pop_front() {
            if self
                .tree
                .node(&path)
                .is_some_and(|n| n.directory && !n.expanded)
            {
                self.expand(path, window, cx);
                return;
            }
        }
        if let Some(path) = self.tree_cursor.take() {
            self.files.select_path(&path);
            self.scroll.scroll_to_item(
                self.files.selected_row().unwrap_or(0),
                ScrollStrategy::Nearest,
            );
        }
    }
    pub(super) fn expand(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading()
            || self
                .tree
                .node(&path)
                .is_none_or(|n| !n.directory || n.expanded)
        {
            return;
        }
        let Some(mut location) = self.location.clone() else {
            return;
        };
        location.directory = location.root.base().join(&path);
        self.selection_changed(false, cx);
        let id = self.id();
        self.tree_pending = Some(path.clone());
        let operation = self.service.read_directory(
            self.store.clone(),
            location,
            id,
            self.show_hidden,
            self.show_ignored,
        );
        self.listing_cancel = Some(operation.cancel);
        self.status(format!("Loading {path}…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.id() != id {
                    return;
                }
                this.listing_cancel = None;
                if this.tree_pending.take().as_deref() != Some(&path) {
                    this.restore_tree(window, cx);
                    cx.notify();
                    return;
                }
                match result {
                    Ok(Ok(directory)) => {
                        if this.tree.expand(
                            &path,
                            directory
                                .entries
                                .iter()
                                .map(|e| (e.path.to_string_lossy().into_owned(), e.directory)),
                        ) {
                            this.entries.extend(directory.entries);
                            this.rebuild_tree(cx);
                            this.status(format!("{} entries", this.files.len()), cx);
                        }
                    }
                    Ok(Err(error)) => {
                        this.tree_restore.clear();
                        this.tree_cursor = None;
                        this.status(error.to_string(), cx);
                    }
                    Err(error) => {
                        this.tree_restore.clear();
                        this.tree_cursor = None;
                        this.status(format!("Background task failed: {error}"), cx);
                    }
                }
                this.restore_tree(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn toggle_tree(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.browser_focus.focus(window, cx);
        self.files.select_path(&path);
        if self.tree_pending.as_deref() == Some(&path) {
            self.stop_listing(cx);
            self.status(format!("Collapsed {path}"), cx);
        } else if self.tree.node(&path).is_some_and(|n| n.expanded) {
            self.tree.collapse(&path);
            self.entries
                .retain(|e| self.tree.node(&e.path.to_string_lossy()).is_some());
            self.rebuild_tree(cx);
            self.files.select_path(&path);
            self.status(format!("Collapsed {path}"), cx);
        } else {
            self.expand(path, window, cx);
        }
        self.selection_changed(false, cx);
        cx.notify();
    }
    pub(super) fn collapse(&mut self, _: &Collapse, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser_focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let path = self.files.selected().map(str::to_owned);
        if let Some(path) = path {
            if self.tree_pending.as_deref() == Some(&path)
                || self.tree.node(&path).is_some_and(|n| n.expanded)
            {
                self.toggle_tree(path, window, cx);
                return;
            }
            if let Some(parent) = self.tree.parent(&path).map(str::to_owned) {
                self.toggle_tree(parent, window, cx);
                return;
            }
            return;
        }
        self.up(window, cx);
    }
}
