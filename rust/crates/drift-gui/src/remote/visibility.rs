use super::*;

impl RemotePane {
    pub(super) fn start_visibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visibility_revision += 1;
        self.menu_revision += 1;
        if let Some(cancel) = self.visibility_cancel.take() {
            cancel.cancel();
        }
        self.visibility = None;
        self.directory_entries.retain(|path, _| {
            self.files.is_marked(path) && !self.entries.iter().any(|entry| entry.path == *path)
        });
        self.directory_entries.extend(
            self.entries
                .iter()
                .filter(|entry| entry.directory)
                .map(|entry| (entry.path.clone(), entry.clone())),
        );
        let (Some(location), Some(host), Some(session)) = (
            self.selection_location.clone(),
            self.target.clone(),
            self.session.clone(),
        ) else {
            self.visibility = Some(Default::default());
            self.set_entries(cx);
            self.restore_tree(window, cx);
            return;
        };
        let id = OperationId {
            project: self.project,
            operation: self.operation,
        };
        let connection = self.connection;
        let revision = self.visibility_revision;
        let mut entries = self.entries.clone();
        entries.extend(
            self.directory_entries
                .values()
                .filter(|entry| {
                    self.files.is_marked(&entry.path)
                        && !self
                            .entries
                            .iter()
                            .any(|current| current.path == entry.path)
                })
                .cloned(),
        );
        let operation = self
            .service
            .classify_entries(location, host, session, entries, id);
        self.visibility_cancel = Some(operation.cancel);
        self.status("Classifying remote paths…".into(), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.project != id.project
                    || this.connection != connection
                    || this.operation != id.operation
                    || this.visibility_revision != revision
                {
                    return;
                }
                this.visibility_cancel = None;
                let selection = this.files.selected().map(str::to_owned);
                let marks = this.files.marked();
                match result {
                    Ok(Ok(visibility)) => {
                        this.visibility = Some(visibility);
                        this.set_entries(cx);
                        this.status(format!("{} remote entries", this.files.len()), cx);
                        if this.listing.is_none() {
                            this.restore_tree(window, cx);
                        }
                    }
                    Ok(Err(error)) => {
                        this.tree_restore.clear();
                        this.tree_cursor = None;
                        this.set_entries(cx);
                        this.status(error.to_string(), cx);
                    }
                    Err(error) => {
                        this.tree_restore.clear();
                        this.tree_cursor = None;
                        this.set_entries(cx);
                        this.status(format!("Remote classification task failed: {error}"), cx);
                    }
                }
                if this.files.selected() != selection.as_deref() || this.files.marked() != marks {
                    this.selection_changed(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn toggle_ignored(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading()
            || !self.has_session()
            || self.selection_location.is_none()
            || self.visibility.is_none()
        {
            return;
        }
        self.dismiss_context_menu(window, cx);
        self.show_ignored = !self.show_ignored;
        self.set_entries(cx);
        self.selection_changed(cx);
        self.status(format!("{} remote entries", self.files.len()), cx);
        cx.notify();
    }
}
