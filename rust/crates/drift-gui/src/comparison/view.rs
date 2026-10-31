use super::*;
use crate::design::{Palette, Text, control, metric, size, space};
use drift_app::sync::{ItemOutcome, StopReason};
use gpui_kit::TestSupportExt;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::prelude::FluentBuilder;
impl Render for ComparisonPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::current(cx);
        let files_owner = cx.entity();
        div()
            .id("comparison")
            .key_context("Drift DriftComparison")
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .text_color(palette.text)
            .bg(palette.canvas)
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
                    .id("comparison-toolbar")
                    .test_support()
                    .flex()
                    .flex_wrap()
                    .flex_shrink_0()
                    .items_center()
                    .min_h(metric(size::HEADER, cx))
                    .text_size(Text::Body.rems())
                    .gap(metric(space::CONTROL, cx))
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .bg(palette.chrome)
                    .border_b_1()
                    .border_color(palette.border)
                    .child(div().text_size(Text::Title.rems()).font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Comparison"))
                    .child(
                        control(Button::new("comparison-back"), cx)
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
                        control(Button::new("comparison-refresh"), cx)
                            .label("Refresh comparison")
                            .disabled(self.is_loading() || self.request.as_ref().is_none_or(|r| r.connection.state() != drift_core::remote::ConnectionState::Connected))
                            .on_click(cx.listener(|this, _, w, cx| this.refresh(&Refresh, w, cx))),
                    )
                    .child(
                        control(Button::new("comparison-ignored"), cx)
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
                        control(Button::new("comparison-progress"), cx)
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
            )
            .child(
                div()
                    .id("comparison-plan-toolbar")
                    .test_support()
                    .flex()
                    .flex_wrap()
                    .flex_shrink_0()
                    .items_center()
                    .min_w_0()
                    .min_h(metric(size::HEADER, cx))
                    .text_size(Text::Body.rems())
                    .gap(metric(space::CONTROL, cx))
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .border_b_1()
                    .border_color(palette.border)
                    .child(div().text_size(Text::Metadata.rems()).text_color(palette.muted).child("Plan"))
                    .child(
                        control(Button::new("comparison-action"), cx)
                            .tooltip("Change action for the selected file")
                            .label(self.selected.map_or("Skip", |i| self.decisions[i].label()))
                            .disabled(
                                self.is_loading() || self.confirm_sync.is_some()
                                    || self.selected.is_none_or(|i| {
                                        self.session.as_ref().unwrap().entries[i].error.is_some()
                                    }),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.cycle(w, cx))),
                    )
                    .child(control(Button::new("sync-selected"), cx).secondary().label("Sync selected")
                        .disabled(!self.can_sync() || self.selected.is_none_or(|i| self.decisions[i] == Decision::Skip))
                        .on_click(cx.listener(|this, _, _, cx| this.prepare_sync(false, cx))))
                    .child(control(Button::new("sync-all"), cx).secondary().label("Sync all actions")
                        .disabled(!self.can_sync() || self.decisions.iter().all(|d| *d == Decision::Skip))
                        .on_click(cx.listener(|this, _, _, cx| this.prepare_sync(true, cx)))),
            )
            .when_some(self.confirm_sync.as_ref(), |view, decisions| {
                let uploads = decisions.iter().filter(|d| **d == Decision::Upload).count();
                let downloads = decisions.iter().filter(|d| **d == Decision::Download).count();
                let deletes = decisions.iter().filter(|d| matches!(d, Decision::DeleteLocal | Decision::DeleteRemote)).count();
                view.child(
                    div()
                        .id("sync-confirmation")
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .min_w_0()
                        .text_size(Text::Body.rems())
                        .px(metric(space::PANEL, cx))
                        .py(metric(space::CONTROL, cx))
                        .gap(metric(space::CONTROL, cx))
                        .border_b_1()
                        .border_color(palette.border)
                        .child(format!("Run {uploads} uploads, {downloads} downloads and {deletes} deletions in the comparison scope?"))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(metric(space::CONTROL, cx))
                                .child(Button::new("sync-confirm").primary().label("Run sync").on_click(cx.listener(|this, _, w, cx| this.start_sync(w, cx))))
                                .child(Button::new("sync-dismiss").label("Cancel").on_click(cx.listener(|this, _, _, cx| { this.confirm_sync = None; cx.notify(); }))),
                        ),
                )
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
                view.child(div().id("sync-report").text_size(Text::Body.rems()).px(metric(space::PANEL, cx)).py(metric(space::CONTROL, cx))
                    .child(div().flex().flex_wrap().items_center().gap(metric(space::CONTROL, cx))
                        .child(format!("Sync: {completed} completed, {} errors{suffix}", errors.len()))
                        .when(!errors.is_empty(), |view| view.child(control(Button::new("sync-errors-toggle"), cx)
                            .label(if self.show_errors { "Hide errors" } else { "Show errors" })
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
                    .id("comparison-status")
                    .flex_shrink_0()
                    .px(metric(space::PANEL, cx))
                    .py(metric(space::TIGHT, cx))
                    .text_size(Text::Metadata.rems())
                    .text_color(palette.muted)
                    .border_b_1()
                    .border_color(palette.border)
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
                        gpui_kit::canvas(
                            move |bounds, window, cx| {
                                let mut content = files_owner.update(cx, |pane, cx| {
                                    let compact = bounds.size.width < metric(240., cx);
                                    let padding = metric(if compact { space::TIGHT } else { space::PANEL }, cx);
                                    let row_height = if compact {
                                        let content_width = (bounds.size.width - padding * 2. - metric(2., cx)).max(px(1.));
                                        let actions = [Decision::Skip, Decision::Upload, Decision::Download, Decision::DeleteLocal, Decision::DeleteRemote].map(Decision::label);
                                        // Uniform rows must fit every status/action, not only the measured first file.
                                        let heights = [(["Changed", "Local only", "Remote only", "Error"]).as_slice(), actions.as_slice()].map(|labels| {
                                            labels.iter().map(|label| {
                                                let mut text = div().w(content_width).text_size(Text::Metadata.rems()).line_height(metric(16., cx)).child(*label).into_any_element();
                                                text.layout_as_root(gpui_kit::size(gpui_kit::AvailableSpace::Definite(content_width), gpui_kit::AvailableSpace::MinContent), window, cx).height
                                            }).max().unwrap()
                                        });
                                        metric(80., cx).max(metric(20. + 2. * space::TIGHT, cx) + heights[0] + heights[1])
                                    } else {
                                        metric(size::ROW * 2., cx)
                                    };
                                    // Keep the fractional file anchor when density or typography changes.
                                    if pane.file_row_height > px(0.) && pane.file_row_height != row_height {
                                        let scroll = pane.scroll.0.borrow();
                                        let offset = scroll.base_handle.offset();
                                        scroll.base_handle.set_offset(gpui_kit::point(offset.x, offset.y * (row_height / pane.file_row_height)));
                                    }
                                    pane.file_row_height = row_height;
                        div()
                            .id("comparison-files")
                            .test_support()
                            .flex()
                            .flex_col()
                            .size_full()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .id("comparison-files-header")
                                    .test_support()
                                    .flex_shrink_0()
                                    .px(padding)
                                    .py(metric(space::TIGHT, cx))
                                    .min_h(metric(size::HEADER, cx))
                                    .min_w_0()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(metric(space::TIGHT, cx))
                                    .text_size(Text::Metadata.rems())
                                    .line_height(metric(16., cx))
                                    .text_color(palette.muted)
                                    .bg(palette.chrome)
                                    .border_b_1()
                                    .border_color(palette.border)
                                    .child(div().min_w_0().max_w_full().child("Files"))
                                    .child(div().id("comparison-shown-count").test_support().aria_label(format!("{} shown", pane.files.len())).min_w_0().max_w_full().child(format!("{} shown", pane.files.len())))
                                    .child(div().id("comparison-total-count").test_support().aria_label(format!("{} total", pane.session.as_ref().map_or(0, |s| s.entries.len()))).min_w_0().max_w_full().child(format!("{} total", pane.session.as_ref().map_or(0, |s| s.entries.len())))),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .px(padding)
                                    .py(metric(space::CONTROL, cx))
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
                                    .child(crate::form_input::guard(&pane.filter, |input| input.id("comparison-filter"))),
                            )
                            .child(
                                div()
                                    .id("comparison-file-list")
                                    .test_support()
                                    .key_context("DriftComparisonFiles")
                                    .track_focus(&pane.focus)
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .border_l(metric(2., cx))
                                    .border_color(palette.border)
                                    .focus(move |style| style.border_color(palette.focus))
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
                                            pane.files.len(),
                                            cx.processor(
                                                move |this, range: std::ops::Range<usize>, _, cx| {
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
                                                            let chosen = this.selected == Some(i);
                                                            let element = div()
                                                                .id(("comparison-file", row))
                                                                .aria_label(format!("{path} · {status} · {}", this.decisions[i].label()))
                                                                .tooltip({
                                                                    let details = format!("{path}\n{status} · {}", this.decisions[i].label());
                                                                    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(details.clone()).build(window, cx)
                                                                })
                                                                .h(row_height)
                                                                .px(padding)
                                                                .py(metric(space::TIGHT, cx))
                                                                .flex()
                                                                .flex_col()
                                                                .justify_center()
                                                                .min_w_0()
                                                                .overflow_hidden()
                                                                .bg(if chosen {
                                                                    palette.selected
                                                                } else {
                                                                    palette.canvas
                                                                })
                                                                .hover(move |style| style.bg(if chosen { palette.selected } else { palette.hover }))
                                                                .child(div().text_size(Text::Body.rems()).line_height(metric(20., cx)).truncate().child(path.into_owned()))
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .when(compact, |view| view.flex_col())
                                                                        .min_w_0()
                                                                        .text_size(Text::Metadata.rems())
                                                                        .line_height(metric(16., cx))
                                                                        .text_color(if entry.error.is_some() { cx.theme().danger } else { palette.muted })
                                                                        .child(div().id(("comparison-file-status", row)).test_support().min_w_0().max_w_full().whitespace_normal().child(status))
                                                                        .child(div().id(("comparison-file-action", row)).test_support().min_w_0().max_w_full().whitespace_normal().child(if compact { this.decisions[i].label().to_owned() } else { format!(" · {}", this.decisions[i].label()) })),
                                                                )
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
                                        .track_scroll(&pane.scroll)
                                        .flex_1()
                                        .min_h_0(),
                                    ),
                            ).into_any_element()
                                });
                                content.layout_as_root(gpui_kit::size(
                                    gpui_kit::AvailableSpace::Definite(bounds.size.width),
                                    gpui_kit::AvailableSpace::Definite(bounds.size.height),
                                ), window, cx);
                                content.prepaint_at(bounds.origin, window, cx);
                                content
                            },
                            |_, mut content, window, cx| content.paint(window, cx),
                        ).size_full().into_any_element(),
                        self.diff.clone().into_any_element(), window, cx),
                    ),
            )
    }
}
