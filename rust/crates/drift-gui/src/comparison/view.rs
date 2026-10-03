use super::*;
use drift_app::sync::{ItemOutcome, StopReason};
use gpui_kit::prelude::FluentBuilder;
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
impl Render for ComparisonPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("comparison")
            .key_context("Drift DriftComparison")
            .flex()
            .flex_col()
            .size_full()
            .capture_action(cx.listener(|this, _: &Cancel, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .capture_action(cx.listener(|this, _: &gpui_kit::base::actions::Cancel, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .capture_action(cx.listener(|this, _: &gpui_kit::component::input::Escape, w, cx| {
                this.split.cancel_resize(w, cx);
            }))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(|this, _: &CancelWork, w, cx| {
                if this.is_loading() {
                    this.show_errors = false;
                    this.cancel(&Cancel, w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ResizePaneLeft, w, cx| {
                this.split.command(SplitCommand::Left, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResizePaneRight, w, cx| {
                this.split.command(SplitCommand::Right, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResetPaneSizes, w, cx| {
                this.split.command(SplitCommand::Reset, w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleSyncErrors, _, cx| this.toggle_sync_errors(cx)))
            .on_action(cx.listener(|this, _: &Activate, w, cx| this.cycle(w, cx)))
            .on_action(cx.listener(|this, _: &Upload, _, cx| this.direct_sync(Decision::Upload, cx)))
            .on_action(cx.listener(|this, _: &Download, _, cx| this.direct_sync(Decision::Download, cx)))
            .on_action(cx.listener(|this, _: &CycleAll, _, cx| this.cycle_all(cx)))
            .on_action(cx.listener(|this, _: &NextFile, w, cx| this.cursor(true, w, cx)))
            .on_action(cx.listener(|this, _: &PreviousFile, w, cx| this.cursor(false, w, cx)))
            .on_action(cx.listener(|this, _: &ToggleIgnored, w, cx| this.toggle_ignored(w, cx)))
            .on_action(cx.listener(|this, _: &SyncSelected, _, cx| this.prepare_sync(false, cx)))
            .on_action(cx.listener(|this, _: &SyncAll, _, cx| this.prepare_sync(true, cx)))
            .on_action(cx.listener(|this, _: &ConfirmSync, w, cx| this.start_sync(w, cx)))
            .on_action(cx.listener(|this, _: &SwitchPane, w, cx| {
                if this.focus.is_focused(w) {
                    this.diff.focus_handle(cx).focus(w, cx);
                } else {
                    this.focus.focus(w, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &FocusFilter, w, cx| {
                    this.filter.focus_handle(cx).focus(w, cx)
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .p_2()
                    .child(
                        Button::new("comparison-back")
                            .label(if self.show_errors {
                                "Close error details"
                            } else if self.syncing.is_some() {
                                "Cancel sync"
                            } else if self.is_loading() {
                                "Cancel comparison"
                            } else {
                                "Back to browser"
                            })
                            .on_click(cx.listener(|this, _, w, cx| this.cancel(&Cancel, w, cx))),
                    )
                    .child(
                        Button::new("comparison-refresh")
                            .label("Refresh comparison")
                            .disabled(self.is_loading() || self.request.as_ref().is_none_or(|r| r.connection.state() != drift_core::remote::ConnectionState::Connected))
                            .on_click(cx.listener(|this, _, w, cx| this.refresh(&Refresh, w, cx))),
                    )
                    .child(
                        Button::new("comparison-ignored")
                            .label(
                                if self
                                    .request
                                    .as_ref()
                                    .is_some_and(|r| r.scope.include_ignored)
                                {
                                    "Exclude ignored"
                                } else {
                                    "Include ignored"
                                },
                            )
                            .disabled(self.is_loading() || self.confirm_sync.is_some())
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.toggle_ignored(w, cx);
                            })),
                    )
                    .child(
                        Button::new("comparison-progress")
                            .label(if self.hide_progress {
                                "Show progress"
                            } else {
                                "Hide progress"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hide_progress = !this.hide_progress;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("comparison-action")
                            .label(self.selected.map_or("Skip", |i| self.decisions[i].label()))
                            .disabled(
                                self.is_loading() || self.confirm_sync.is_some()
                                    || self.selected.is_none_or(|i| {
                                        self.session.as_ref().unwrap().entries[i].error.is_some()
                                    }),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.cycle(w, cx))),
                    )
                    .child(Button::new("sync-selected").label("Sync selected")
                        .disabled(!self.can_sync() || self.selected.is_none_or(|i| self.decisions[i] == Decision::Skip))
                        .on_click(cx.listener(|this, _, _, cx| this.prepare_sync(false, cx))))
                    .child(Button::new("sync-all").label("Sync all actions")
                        .disabled(!self.can_sync() || self.decisions.iter().all(|d| *d == Decision::Skip))
                        .on_click(cx.listener(|this, _, _, cx| this.prepare_sync(true, cx)))),
            )
            .when_some(self.confirm_sync.as_ref(), |view, decisions| {
                let uploads = decisions.iter().filter(|d| **d == Decision::Upload).count();
                let downloads = decisions.iter().filter(|d| **d == Decision::Download).count();
                let deletes = decisions.iter().filter(|d| matches!(d, Decision::DeleteLocal | Decision::DeleteRemote)).count();
                view.child(div().p_2().flex().gap_2()
                    .child(format!("Run {uploads} uploads, {downloads} downloads and {deletes} deletions in the comparison scope? Ctrl/Cmd+Enter to run, Escape to cancel."))
                    .child(Button::new("sync-confirm").label("Run sync").on_click(cx.listener(|this, _, w, cx| this.start_sync(w, cx))))
                    .child(Button::new("sync-dismiss").label("Cancel").on_click(cx.listener(|this, _, _, cx| { this.confirm_sync = None; cx.notify(); }))))
            })
            .when_some(self.sync_result.as_ref(), |view, report| {
                let completed = report.outcomes.iter().filter(|o| **o == ItemOutcome::Completed).count();
                let suffix = match &report.stopped {
                    None => "",
                    Some(StopReason::Cancelled) => "; cancelled",
                    Some(StopReason::ConnectionLost(_)) => "; connection lost — reconnect and compare again",
                    Some(StopReason::TimedOut) => "; timed out — reconnect and compare again",
                };
                let errors: Vec<_> = report.items.iter().zip(&report.outcomes).filter_map(|(item, outcome)| {
                    let (label, reason) = match outcome { ItemOutcome::Failed(e) => ("Failed",e), ItemOutcome::Unknown(e) => ("Outcome unknown",e), _ => return None };
                    Some(format!("{label}: {} {} — {reason}", item.decision.label(), item.local.display()))
                }).collect();
                view.child(div().id("sync-report").p_2()
                    .child(div().flex().gap_2()
                        .child(format!("Sync: {completed} completed, {} errors{suffix}", errors.len()))
                        .when(!errors.is_empty(), |view| view.child(Button::new("sync-errors-toggle")
                            .label(if self.show_errors { "Hide errors (e)" } else { "Show errors (e)" })
                            .disabled(self.confirm_sync.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_sync_errors(cx))))))
                    .when(self.show_errors, |view| view.child(div().id("sync-error-details")
                        .max_h(px(160.)).overflow_y_scroll()
                        .map(|view| {
                            #[cfg(test)] { view.test_support() }
                            #[cfg(not(test))] { view }
                        })
                        .children(errors.into_iter().map(|error| div().text_color(cx.theme().danger).child(error))))))
            })
            .child(
                div()
                    .p_2()
                    .child(if self.is_loading() && !self.hide_progress {
                        format!(
                            "{}: {} / {} files, {} bytes read",
                            self.progress.phase,
                            self.progress.completed,
                            self.progress.total,
                            self.progress.bytes
                        )
                    } else {
                        if self.is_loading() {
                            if self.syncing.is_some() { "Sync running; progress hidden".into() } else { "Comparison running; progress hidden".into() }
                        } else {
                            if self.stale { "Compare again before syncing".into() } else { "Choose actions, then sync selected or all actions".into() }
                        }
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.split.view("comparison-split",
                        div()
                            .id("comparison-files")
                            .flex()
                            .flex_col()
                            .w_full()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .key_context("DriftComparisonFilter")
                                    .on_action(cx.listener(|this, _: &FocusResults, w, cx| {
                                        this.focus.focus(w, cx);
                                    }))
                                    .on_action(cx.listener(|this, _: &Cancel, w, cx| {
                                        this.focus.focus(w, cx);
                                    }))
                                    .on_action(cx.listener(|this, _: &gpui_kit::component::input::Escape, w, cx| {
                                        this.focus.focus(w, cx);
                                    }))
                                    .child(Input::new(&self.filter).id("comparison-filter")),
                            )
                            .child(
                                div()
                                    .id("comparison-file-list")
                                    .key_context("DriftComparisonFiles")
                                    .track_focus(&self.focus)
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .on_action(cx.listener(|this, _: &CursorFirst, w, cx| this.cursor_edge(false, w, cx)))
                                    .on_action(cx.listener(|this, _: &CursorLast, w, cx| this.cursor_edge(true, w, cx)))
                                    .on_action(cx.listener(|this, _: &CursorUp, w, cx| {
                                        this.cursor(false, w, cx)
                                    }))
                                    .on_action(cx.listener(|this, _: &CursorDown, w, cx| {
                                        this.cursor(true, w, cx)
                                    }))
                                    .on_action(
                                        cx.listener(|this, _: &Activate, w, cx| this.cycle(w, cx)),
                                    )
                                    .child(
                                        uniform_list(
                                            "comparison-list",
                                            self.files.len(),
                                            cx.processor(
                                                |this, range: std::ops::Range<usize>, _, cx| {
                                                    range
                                                        .map(|row| {
                                                            let i = this.files[row];
                                                            let entry = &this
                                                                .session
                                                                .as_ref()
                                                                .unwrap()
                                                                .entries[i];
                                                            let path = entry
                                                                .local
                                                                .strip_prefix(
                                                                    this.request
                                                                        .as_ref()
                                                                        .unwrap()
                                                                        .location
                                                                        .root
                                                                        .base(),
                                                                )
                                                                .unwrap_or(&entry.local)
                                                                .to_string_lossy();
                                                            let path = if path.is_empty() {
                                                                std::borrow::Cow::Borrowed(
                                                                    entry.remote.as_str(),
                                                                )
                                                            } else {
                                                                path
                                                            };
                                                            let status = if entry.error.is_some() {
                                                                "Error"
                                                            } else if entry
                                                                .result
                                                                .as_ref()
                                                                .unwrap()
                                                                .local
                                                                .is_none()
                                                            {
                                                                "Remote only"
                                                            } else if entry
                                                                .result
                                                                .as_ref()
                                                                .unwrap()
                                                                .remote
                                                                .is_none()
                                                            {
                                                                "Local only"
                                                            } else {
                                                                "Changed"
                                                            };
                                                            let element = div()
                                                                .id(("comparison-file", row))
                                                                .h(px(48.))
                                                                .p_2()
                                                                .bg(if this.selected == Some(i) {
                                                                    cx.theme().selection
                                                                } else {
                                                                    cx.theme().background
                                                                })
                                                                .child(format!(
                                                                    "{path}\n{status} · {}",
                                                                    this.decisions[i].label()
                                                                ))
                                                                .on_click(cx.listener(
                                                                    move |this, _, w, cx| {
                                                                        this.focus.focus(w, cx);
                                                                        this.choose(Some(i), w, cx);
                                                                    },
                                                                ));
                                                            #[cfg(test)]
                                                            let element = element.test_support();
                                                            element
                                                        })
                                                        .collect()
                                                },
                                            ),
                                        )
                                        .track_scroll(&self.scroll)
                                        .flex_1()
                                        .min_h_0(),
                                    ),
                            ).into_any_element(),
                        self.diff.clone().into_any_element(), window, cx),
                    ),
            )
    }
}
