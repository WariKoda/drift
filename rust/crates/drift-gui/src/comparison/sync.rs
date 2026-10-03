use super::*;
use drift_app::sync::ItemOutcome;
use drift_core::remote::ConnectionState;

impl ComparisonPane {
    pub(super) fn can_sync(&self) -> bool {
        !self.is_loading()
            && !self.stale
            && self.confirm_sync.is_none()
            && self
                .session
                .as_ref()
                .is_some_and(|s| s.request.connection.state() == ConnectionState::Connected)
    }
    pub(super) fn prepare_sync(&mut self, all: bool, cx: &mut Context<Self>) {
        if !self.can_sync() {
            return;
        }
        let decisions: Vec<_> = self
            .decisions
            .iter()
            .enumerate()
            .map(|(i, &decision)| {
                if all || self.selected == Some(i) {
                    decision
                } else {
                    Decision::Skip
                }
            })
            .collect();
        if decisions.iter().all(|d| *d == Decision::Skip) {
            return;
        }
        self.show_errors = false;
        self.confirm_sync = Some(decisions);
        cx.notify();
    }
    pub(super) fn start_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(decisions) = self.confirm_sync.take() else {
            return;
        };
        if self.is_loading() || self.stale {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        self.id.operation += 1;
        let id = self.id;
        let operation = match self.sync_service.run(session, &decisions, id) {
            Ok(operation) => operation,
            Err(error) => {
                self.status(error.to_string(), cx);
                cx.notify();
                return;
            }
        };
        self.stale = true;
        self.sync_result = None;
        self.show_errors = false;
        self.syncing = Some(operation.operation.cancel);
        self.progress = operation.progress.borrow().clone();
        let stop = CancellationToken::new();
        self.progress_stop = Some(stop.clone());
        let mut progress = operation.progress;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let update = tokio::select! { _ = stop.cancelled() => break, update = progress.changed() => update };
                if update.is_err() { break; }
                let current = progress.borrow_and_update().clone();
                if this.update_in(cx, |this, _, cx| {
                    if this.id == id && this.syncing.is_some() { this.progress = current; cx.notify(); }
                }).is_err() { break; }
            }
        }).detach();
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.operation.task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.id != id {
                    return;
                }
                this.syncing = None;
                if let Some(stop) = this.progress_stop.take() {
                    stop.cancel();
                }
                match result {
                    Ok(Ok(report)) => {
                        let completed = report
                            .outcomes
                            .iter()
                            .filter(|o| **o == ItemOutcome::Completed)
                            .count();
                        let failures = report
                            .outcomes
                            .iter()
                            .filter(|o| {
                                matches!(o, ItemOutcome::Failed(_) | ItemOutcome::Unknown(_))
                            })
                            .count();
                        if completed > 0
                            && let Some(request) = &this.request
                        {
                            cx.emit(ComparisonEvent::FilesChanged {
                                project: this.id.project,
                                connection: request.connection.id,
                            });
                        }
                        let rebuild = report.stopped.is_none()
                            && this.request.as_ref().is_some_and(|r| {
                                r.connection.state() == ConnectionState::Connected
                            });
                        this.status(
                            format!(
                                "Sync: {completed} completed, {failures} errors{}",
                                if rebuild {
                                    "; comparing again"
                                } else {
                                    "; reconnect and compare again before syncing"
                                }
                            ),
                            cx,
                        );
                        this.sync_result = Some(report);
                        if rebuild {
                            this.load(window, cx);
                        }
                    }
                    Ok(Err(error)) => this.status(error.to_string(), cx),
                    Err(error) => this.status(
                        format!("Sync task failed: {error}; compare again before syncing"),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
        self.status("Syncing…".into(), cx);
        cx.notify();
    }
}
