//! Comparison controls and file list. Transport/scanning live in drift-app;
//! the unified renderer has its own view and per-file folding/scroll state.
use crate::{
    actions::*,
    diff::{DiffPane, DiffState},
    pane_split::{PaneSplit, SplitCommand},
};
use drift_app::{
    browser::{BrowserService, OperationId},
    comparison::{ComparisonService, ComparisonSession, LoadRequest, Progress},
    sync::{SyncResult, SyncService},
};
use drift_core::diff::Decision;
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    input::{InputEvent, InputState},
};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollStrategy, StatefulInteractiveElement, Styled,
    Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;

pub enum ComparisonEvent {
    FilesChanged {
        project: u64,
        connection: OperationId,
    },
    Status {
        project: u64,
        connection: OperationId,
        message: String,
    },
    Closed,
}
impl EventEmitter<ComparisonEvent> for ComparisonPane {}
pub struct ComparisonPane {
    service: ComparisonService,
    sync_service: SyncService,
    request: Option<LoadRequest>,
    pub session: Option<ComparisonSession>,
    id: OperationId,
    loading: Option<CancellationToken>,
    syncing: Option<CancellationToken>,
    stale: bool,
    sync_result: Option<SyncResult>,
    show_errors: bool,
    confirm_sync: Option<Vec<Decision>>,
    progress_stop: Option<CancellationToken>,
    progress: Progress,
    hide_progress: bool,
    visible: bool,
    filter: Entity<InputState>,
    files: Vec<usize>,
    selected: Option<usize>,
    decisions: Vec<Decision>,
    states: BTreeMap<usize, DiffState>,
    diff: Entity<DiffPane>,
    split: PaneSplit,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _subscription: Subscription,
}
impl Focusable for ComparisonPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Drop for ComparisonPane {
    fn drop(&mut self) {
        if let Some(token) = self.loading.take() {
            token.cancel();
        }
        if let Some(token) = self.syncing.take() {
            token.cancel();
        }
        if let Some(token) = self.progress_stop.take() {
            token.cancel();
        }
    }
}
impl ComparisonPane {
    pub fn new(service: BrowserService, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter comparison files…"));
        let subscription = cx.subscribe_in(&filter, window, |this, state, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                let query = state.read(cx).value().to_lowercase();
                this.files = this.session.as_ref().map_or_else(Vec::new, |s| {
                    s.entries
                        .iter()
                        .enumerate()
                        .filter_map(|(i, e)| {
                            (e.local
                                .strip_prefix(s.request.location.root.base())
                                .unwrap_or(&e.local)
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&query)
                                || e.remote
                                    .strip_prefix(&s.request.connection.root)
                                    .unwrap_or(&e.remote)
                                    .to_lowercase()
                                    .contains(&query))
                            .then_some(i)
                        })
                        .collect()
                });
                if this.selected.is_some_and(|i| !this.files.contains(&i)) {
                    this.choose(None, window, cx);
                }
                cx.notify();
            }
        });
        Self {
            service: ComparisonService::new(service.clone()),
            sync_service: SyncService::new(service),
            request: None,
            session: None,
            id: OperationId {
                project: 0,
                operation: 0,
            },
            loading: None,
            syncing: None,
            stale: true,
            sync_result: None,
            show_errors: false,
            confirm_sync: None,
            progress_stop: None,
            progress: Progress {
                phase: "Scanning",
                completed: 0,
                total: 0,
                bytes: 0,
            },
            hide_progress: false,
            visible: false,
            filter,
            files: vec![],
            selected: None,
            decisions: vec![],
            states: BTreeMap::new(),
            diff: cx.new(DiffPane::new),
            split: PaneSplit::new(Some(px(350.)), cx),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle().tab_stop(true),
            _subscription: subscription,
        }
    }
    pub fn deactivate_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.split.deactivate(window, cx);
        self.diff.update(cx, |diff, _| diff.deactivate());
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn is_loading(&self) -> bool {
        self.loading.is_some() || self.syncing.is_some()
    }
    #[cfg(test)]
    pub(crate) fn diff_for_test(&self) -> &Entity<DiffPane> {
        &self.diff
    }
    #[cfg(test)]
    pub(crate) fn sync_result_for_test(&self) -> Option<&SyncResult> {
        self.sync_result.as_ref()
    }
    pub fn context(
        &mut self,
        project: u64,
        connection: Option<OperationId>,
        cx: &mut Context<Self>,
    ) {
        if self.request.is_some() && project == self.id.project && connection.is_none() {
            // Retain the active sync report when the connection observer detaches
            // the browser. A new connection still requires a new comparison.
            self.stale = true;
            self.confirm_sync = None;
            cx.notify();
            return;
        }
        if self
            .request
            .as_ref()
            .is_some_and(|r| project != self.id.project || Some(r.connection.id) != connection)
        {
            self.invalidate(cx);
        }
    }
    pub fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.id.operation += 1;
        if let Some(token) = self.loading.take() {
            token.cancel();
        }
        if let Some(token) = self.syncing.take() {
            token.cancel();
        }
        if let Some(token) = self.progress_stop.take() {
            token.cancel();
        }
        self.request = None;
        self.session = None;
        self.files.clear();
        self.selected = None;
        self.states.clear();
        self.stale = true;
        self.sync_result = None;
        self.show_errors = false;
        self.confirm_sync = None;
        self.visible = false;
        self.diff.update(cx, |diff, cx| {
            diff.show(None, DiffState::default(), String::new(), cx)
        });
        cx.notify();
    }
    pub fn open(
        &mut self,
        request: LoadRequest,
        project: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split.deactivate(window, cx);
        self.invalidate(cx);
        self.id.project = project;
        self.visible = true;
        self.hide_progress = false;
        self.request = Some(request);
        self.filter
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.focus.focus(window, cx);
        self.load(window, cx);
    }
    fn status(&self, message: String, cx: &mut Context<Self>) {
        if let Some(request) = &self.request {
            cx.emit(ComparisonEvent::Status {
                project: self.id.project,
                connection: request.connection.id,
                message,
            });
        }
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.request.clone() else {
            return;
        };
        if self.is_loading() {
            return;
        }
        self.confirm_sync = None;
        self.stale = true;
        self.id.operation += 1;
        let id = self.id;
        let operation = self.service.load(request, id);
        self.loading = Some(operation.operation.cancel);
        self.progress = operation.progress.borrow().clone();
        let stop = CancellationToken::new();
        self.progress_stop = Some(stop.clone());
        let mut progress = operation.progress;
        cx.spawn_in(window,async move |this,cx| {
            loop {
                let update = tokio::select! { _ = stop.cancelled() => break, update = progress.changed() => update };
                if update.is_err() { break; }
                let current = progress.borrow_and_update().clone();
                if this.update_in(cx,|this,_,cx| { if this.id == id && this.loading.is_some() { this.progress = current; cx.notify(); } }).is_err() { break; }
            }
        }).detach();
        cx.spawn_in(window,async move |this,cx| {
            let result = operation.operation.task.await;
            let _ = this.update_in(cx,|this,window,cx| {
                if this.id != id { return; }
                this.loading = None;
                if let Some(stop) = this.progress_stop.take() { stop.cancel(); }
                match result {
                    Ok(Ok(session)) => {
                        this.stale = false;
                        let previous = this.selected.and_then(|i| this.session.as_ref()?.entries.get(i)).map(|e| (e.local.clone(), e.remote.clone()));
                        if let Some(index) = this.selected {
                            this.states.insert(index, this.diff.read(cx).snapshot());
                        }
                        this.decisions = session.entries.iter().map(|entry| if entry.error.is_some() { Decision::Skip } else { entry.result.as_ref().map_or(Decision::Skip,|r| r.suggestion()) }).collect();
                        let mut states = BTreeMap::new();
                        if let Some(old_session) = &this.session {
                            let old_entries: BTreeMap<_, _> = old_session.entries.iter().enumerate()
                                .map(|(index, entry)| ((&entry.local, &entry.remote), (index, entry))).collect();
                            for (index, entry) in session.entries.iter().enumerate() {
                                let Some((old_index, old_entry)) = old_entries.get(&(&entry.local, &entry.remote)) else { continue; };
                                if old_entry.error.is_some() || entry.error.is_some() { continue; }
                                let (Some(old_result), Some(result)) = (&old_entry.result, &entry.result) else { continue; };
                                if old_result.lines != result.lines
                                    || old_result.binary != result.binary
                                    || old_result.content_diff != result.content_diff
                                    || old_result.local.is_some() != result.local.is_some()
                                    || old_result.remote.is_some() != result.remote.is_some()
                                { continue; }
                                if let Some(mut state) = this.states.get(old_index).cloned() {
                                    state.direction = this.decisions[index];
                                    states.insert(index, state);
                                }
                            }
                        }
                        let errors = session.entries.iter().filter(|e| e.error.is_some()).count();
                        let message = format!("{} pairs; {} differences/errors; {errors} errors; {} ignored skipped; {} explicit ignored; {} hidden",session.scope.pairs,session.entries.len(),session.scope.ignored_skipped,session.scope.explicit_ignored,session.scope.hidden);
                        this.session = Some(session); this.states = states; this.selected = None;
                        this.filter.update(cx,|input,cx| input.set_value("",window,cx));
                        this.files = (0..this.session.as_ref().unwrap().entries.len()).collect();
                        let index = previous.and_then(|(local, remote)| this.session.as_ref().unwrap().entries.iter().position(|e| e.local == local && e.remote == remote)).or_else(|| this.files.first().copied());
                        this.choose(index,window,cx);
                        this.status(message,cx);
                    }
                    Ok(Err(error)) => { this.session = None; this.files.clear(); this.selected = None; this.diff.update(cx,|diff,cx| diff.show(None,DiffState::default(),error.to_string(),cx)); this.status(error.to_string(),cx); }
                    Err(error) => this.status(format!("Comparison task failed: {error}"),cx),
                }
                cx.notify();
            });
        }).detach();
        self.status("Scanning and comparing…".into(), cx);
        cx.notify();
    }
    fn choose(&mut self, index: Option<usize>, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(old) = self.selected {
            self.states.insert(old, self.diff.read(cx).snapshot());
        }
        self.selected = index;
        let (result, state, message) =
            if let Some(entry) = index.and_then(|i| self.session.as_ref()?.entries.get(i)) {
                let state = self
                    .states
                    .get(&index.unwrap())
                    .cloned()
                    .unwrap_or(DiffState {
                        direction: self.decisions[index.unwrap()],
                        ..Default::default()
                    });
                (
                    entry.result.as_ref().map(Arc::clone),
                    state,
                    entry
                        .error
                        .clone()
                        .unwrap_or_else(|| "Empty file or no renderable changes".into()),
                )
            } else {
                (
                    None,
                    DiffState::default(),
                    "No differences in this scope".into(),
                )
            };
        self.diff
            .update(cx, |diff, cx| diff.show(result, state, message, cx));
        cx.notify();
    }
    fn cycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading() || self.confirm_sync.is_some() {
            return;
        }
        let Some(index) = self.selected else {
            return;
        };
        let Some(entry) = self.session.as_ref().and_then(|s| s.entries.get(index)) else {
            return;
        };
        if entry.error.is_some() {
            return;
        }
        if let Some(result) = &entry.result {
            self.decisions[index] = result.next_decision(self.decisions[index]);
            self.diff
                .update(cx, |diff, cx| diff.direction(self.decisions[index], cx));
            self.focus.focus(window, cx);
            cx.notify();
        }
    }
    fn direct_sync(&mut self, decision: Decision, cx: &mut Context<Self>) {
        if !self.can_sync() {
            return;
        }
        let Some(index) = self.selected else {
            return;
        };
        let Some(entry) = self.session.as_ref().and_then(|s| s.entries.get(index)) else {
            return;
        };
        if entry.error.is_some()
            || entry.result.as_ref().is_none_or(|result| match decision {
                Decision::Upload => result.local.is_none(),
                Decision::Download => result.remote.is_none(),
                _ => true,
            })
        {
            return;
        }
        self.decisions[index] = decision;
        self.diff
            .update(cx, |diff, cx| diff.direction(decision, cx));
        self.prepare_sync(false, cx);
    }
    fn cycle_all(&mut self, cx: &mut Context<Self>) {
        if self.is_loading() || self.confirm_sync.is_some() {
            return;
        }
        if let Some(session) = &self.session {
            for (index, entry) in session.entries.iter().enumerate() {
                self.decisions[index] = if entry.error.is_some() {
                    Decision::Skip
                } else {
                    entry.result.as_ref().map_or(Decision::Skip, |result| {
                        result.next_decision(self.decisions[index])
                    })
                };
            }
            for (index, state) in &mut self.states {
                state.direction = self.decisions[*index];
            }
            if let Some(index) = self.selected {
                self.diff
                    .update(cx, |diff, cx| diff.direction(self.decisions[index], cx));
            }
            cx.notify();
        }
    }
    pub fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        self.load(window, cx);
    }
    pub fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirm_sync.take().is_some() {
            cx.notify();
            return;
        }
        if self.show_errors {
            self.show_errors = false;
            cx.notify();
            return;
        }
        if let Some(token) = &self.syncing {
            token.cancel();
            self.status(
                "Cancelling sync; waiting for transfer completion…".into(),
                cx,
            );
            return;
        }
        if self.is_loading() {
            self.invalidate(cx);
        } else {
            self.visible = false;
        }
        self.deactivate_layout(window, cx);
        cx.emit(ComparisonEvent::Closed);
        cx.notify();
    }
    pub(super) fn toggle_sync_errors(&mut self, cx: &mut Context<Self>) {
        if self.confirm_sync.is_none()
            && self.sync_result.as_ref().is_some_and(|report| {
                report.outcomes.iter().any(|outcome| {
                    matches!(
                        outcome,
                        drift_app::sync::ItemOutcome::Failed(_)
                            | drift_app::sync::ItemOutcome::Unknown(_)
                    )
                })
            })
        {
            self.show_errors = !self.show_errors;
            cx.notify();
        }
    }
    #[cfg(test)]
    pub(crate) fn errors_visible_for_test(&self) -> bool {
        self.show_errors
    }
    fn toggle_ignored(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading() || self.confirm_sync.is_some() {
            return;
        }
        if let Some(request) = &mut self.request {
            request.scope.include_ignored = !request.scope.include_ignored;
        }
        self.load(window, cx);
    }
    fn cursor_edge(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        let row = if last {
            self.files.len().saturating_sub(1)
        } else {
            0
        };
        self.choose(self.files.get(row).copied(), window, cx);
        self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
    }
    fn cursor(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .selected
            .and_then(|i| self.files.iter().position(|v| *v == i))
            .unwrap_or(0);
        let row = if next {
            (current + 1).min(self.files.len().saturating_sub(1))
        } else {
            current.saturating_sub(1)
        };
        self.choose(self.files.get(row).copied(), window, cx);
        self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
    }
}
mod sync;
#[cfg(test)]
mod tests;
mod view;
