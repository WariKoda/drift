use super::*;
impl ProjectsPanel {
    pub(super) fn request(
        &mut self,
        command: ProjectCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.writing || self.busy {
            return;
        }
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.operation += 1;
        self.writing = !matches!(command, ProjectCommand::Load);
        self.status = if self.writing {
            "Saving project changes…"
        } else {
            "Loading projects…"
        }
        .into();
        let operation = self.service.manage_projects(
            self.store.clone(),
            command,
            OperationId {
                project: 0,
                operation: self.operation,
            },
        );
        let id = operation.id;
        self.cancel = Some(operation.cancel);
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.operation != id.operation {
                    return;
                }
                this.cancel = None;
                this.writing = false;
                match result {
                    Ok(Ok(response)) => {
                        this.registry = response.registry.clone();
                        this.status = response.warning.clone().unwrap_or_else(|| {
                            format!("{} projects", this.registry.projects.len())
                        });
                        if let Some(project) = &response.changed {
                            this.cursor = Some(project.slug.clone());
                            this.form = None;
                            this.delete = None;
                            this.list_focus.focus(window, cx);
                            this.scroll.scroll_to_item(this.cursor_row(cx));
                            cx.emit(ProjectEvent::Changed(Box::new(response)));
                        }
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) => this.status = format!("Project operation failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
