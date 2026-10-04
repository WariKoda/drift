use super::*;

pub(super) struct FinderSnapshot {
    files: FileList,
    tree: FileTree,
    entries: Vec<Entry>,
    query: String,
    scroll: UniformListScrollHandle,
}

impl BrowserPane {
    pub(super) fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.finder {
            self.focus_filter_input(window, cx);
            return;
        }
        if self.is_loading() {
            return;
        }
        let Some(location) = self.location.clone() else {
            return;
        };
        self.dismiss_context_menu(window, cx);
        self.finder_snapshot = Some(FinderSnapshot {
            files: self.files.clone(),
            tree: std::mem::take(&mut self.tree),
            entries: std::mem::take(&mut self.entries),
            query: self.filter.read(cx).value().to_string(),
            scroll: std::mem::replace(&mut self.scroll, UniformListScrollHandle::new()),
        });
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.finder = true;
        self.files.replace_entries(vec![]);
        self.filter
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.stop_listing(cx);
        self.menu_revision += 1;
        let id = self.id();
        let operation = self
            .service
            .find(location, id, self.show_hidden, self.show_ignored);
        self.listing_cancel = Some(operation.cancel);
        self.focus_filter_input(window, cx);
        self.status("Finding project files…".into(), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.id() != id {
                    return;
                }
                this.listing_cancel = None;
                match result {
                    Ok(Ok(entries)) => this.set_entries(entries, false, cx),
                    Ok(Err(error)) => {
                        this.restore_finder_view(window, cx);
                        this.status(error.to_string(), cx);
                    }
                    Err(error) => {
                        this.restore_finder_view(window, cx);
                        this.status(format!("Background task failed: {error}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn return_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restore_finder_view(window, cx) {
            self.browser_focus.focus(window, cx);
        }
    }

    fn restore_finder_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(snapshot) = self.finder_snapshot.take() else {
            return false;
        };
        self.dismiss_context_menu(window, cx);
        self.stop_listing(cx);
        self.finder = false;
        self.tree_restore.clear();
        self.tree_cursor = None;
        self.files.restore_view(snapshot.files);
        self.tree = snapshot.tree;
        self.entries = snapshot.entries;
        self.scroll = snapshot.scroll;
        self.filter
            .update(cx, |state, cx| state.set_value(snapshot.query, window, cx));
        self.menu_revision += 1;
        self.selection_changed(false, cx);
        self.status(format!("{} entries", self.files.len()), cx);
        cx.notify();
        true
    }
}
