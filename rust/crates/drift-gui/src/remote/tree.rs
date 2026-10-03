use super::*;

impl RemotePane {
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
        let Some(session) = self.session.clone() else {
            return;
        };
        self.operation += 1;
        let id = OperationId {
            project: self.project,
            operation: self.operation,
        };
        let connection = self.connection;
        self.tree_pending = Some(path.clone());
        let operation = self.service.list(session, path.clone(), id);
        self.listing = Some(operation.cancel);
        self.selection_changed(cx);
        self.status(format!("Loading {path}…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.project != id.project
                    || this.operation != id.operation
                    || this.connection != connection
                {
                    return;
                }
                this.listing = None;
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
                                .map(|e| (e.path.clone(), e.directory)),
                        ) {
                            this.entries.extend(directory.entries);
                            this.set_entries(cx);
                            this.status(format!("{} remote entries", this.files.len()), cx);
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
        self.focus.focus(window, cx);
        self.files.select_path(&path);
        if self.tree_pending.as_deref() == Some(&path) {
            // Abandon this result, allowing the bounded listing to finish on the
            // existing connection. Explicit Cancel still shuts down remote I/O.
            self.tree_pending = None;
            self.status(format!("Collapsed {path}"), cx);
        } else if self.tree.node(&path).is_some_and(|n| n.expanded) {
            self.tree.collapse(&path);
            self.entries.retain(|e| self.tree.node(&e.path).is_some());
            self.set_entries(cx);
            self.files.select_path(&path);
            self.status(format!("Collapsed {path}"), cx);
        } else {
            self.expand(path, window, cx);
        }
        self.selection_changed(cx);
        cx.notify();
    }
    pub(super) fn collapse(&mut self, _: &Collapse, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
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
        self.up(&GoUp, window, cx);
    }
}
