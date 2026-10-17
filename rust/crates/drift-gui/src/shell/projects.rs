//! Startup and project mutation routing; child views own forms and operations.
use super::*;
use drift_app::projects::ProjectResponse;
impl Shell {
    pub(super) fn resolve_start(
        &mut self,
        options: StartOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let operation = self.service.resolve_start(
            self.store.clone(),
            options,
            OperationId {
                project: 0,
                operation: 0,
            },
        );
        self.startup = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.startup.take().is_none() {
                    return;
                }
                match result {
                    Ok(Ok(start)) => {
                        this.projects.update(cx, |projects, cx| {
                            projects.set_context(start.registry, false, cx)
                        });
                        if start.dashboard {
                            this.projects
                                .update(cx, |projects, cx| projects.show(window, cx));
                            this.status = "Choose a project".into();
                        } else {
                            this.open_project(start.directory, window, cx);
                        }
                    }
                    Ok(Err(error)) => this.status = format!("Startup failed: {error}"),
                    Err(error) => this.status = format!("Startup task failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn close_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.read(cx).visible() {
            return;
        }
        if self.browser.read(cx).location().is_none() {
            self.open_project(self.start_directory.clone(), window, cx);
        } else {
            self.browser
                .update(cx, |browser, cx| browser.focus(window, cx));
        }
    }
    pub(super) fn project_changed(
        &mut self,
        response: &ProjectResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(previous) = &response.changed else {
            return;
        };
        let Some(location) = self.browser.read(cx).location().cloned() else {
            return;
        };
        if location.slug.as_deref() != Some(previous.slug.as_str()) {
            return;
        }
        let current = response.registry.find(&previous.slug);
        if current.is_some_and(|p| p.path == previous.path) {
            return;
        }
        let path = current.map_or(location.directory, |p| p.path.clone());
        self.browser
            .update(cx, |browser, cx| browser.invalidate_project(cx));
        self.preview.update(cx, |preview, cx| {
            preview.select(None, self.browser.read(cx).id().project, window, cx)
        });
        // Keep the dashboard open while rebuilding the active project's root.
        self.browser
            .update(cx, |browser, cx| browser.open(path, window, cx));
        self.projects
            .update(cx, |projects, cx| projects.focus(window, cx));
    }
}
