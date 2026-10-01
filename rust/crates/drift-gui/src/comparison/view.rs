use super::*;
#[cfg(test)]
use gpui_kit::test::TestSupportExt;
impl Render for ComparisonPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("comparison")
            .key_context("Drift")
            .flex()
            .flex_col()
            .size_full()
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cancel))
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
                            .label(if self.is_loading() {
                                "Cancel comparison"
                            } else {
                                "Back to browser"
                            })
                            .on_click(cx.listener(|this, _, w, cx| this.cancel(&Cancel, w, cx))),
                    )
                    .child(
                        Button::new("comparison-refresh")
                            .label("Refresh comparison")
                            .disabled(self.is_loading())
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
                            .disabled(self.is_loading())
                            .on_click(cx.listener(|this, _, w, cx| {
                                if let Some(r) = &mut this.request {
                                    r.scope.include_ignored = !r.scope.include_ignored;
                                }
                                this.load(w, cx);
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
                                self.is_loading()
                                    || self.selected.is_none_or(|i| {
                                        self.session.as_ref().unwrap().entries[i].error.is_some()
                                    }),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.cycle(w, cx))),
                    ),
            )
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
                            "Comparison running; progress hidden".into()
                        } else {
                            "Action preview".into()
                        }
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("comparison-files")
                            .flex()
                            .flex_col()
                            .w(px(350.))
                            .min_h_0()
                            .child(Input::new(&self.filter).id("comparison-filter"))
                            .child(
                                div()
                                    .id("comparison-file-list")
                                    .key_context("DriftComparisonFiles")
                                    .track_focus(&self.focus)
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
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
                            ),
                    )
                    .child(self.diff.clone()),
            )
    }
}
