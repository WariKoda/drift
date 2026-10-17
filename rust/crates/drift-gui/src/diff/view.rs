use super::*;

impl Render for DiffPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.active = true;
        if self.blur.is_none() {
            self.blur = Some(cx.on_blur(&self.focus, window, |this, _, _| this.stop_gesture()));
        }
        self.geometry = Rc::new(RefCell::new(text::Geometry::default()));
        let geometry = self.geometry.clone();
        let caret = self.focus.is_focused(window);
        let revision = self.revision;
        let entity = cx.entity().downgrade();
        div()
            .id("unified-diff")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .key_context("DriftDiff")
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(move |this, _, window, cx| {
                if !this.active || this.revision != revision {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .capture_any_mouse_up(cx.listener(move |this, _, window, cx| {
                if !this.active || this.revision != revision {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(Self::copy))
            .capture_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.drag.is_some() {
                    this.stop_gesture();
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .on_action(
                cx.listener(|this, _: &DiffCaretLeft, _, cx| {
                    this.move_caret(-1, 0, None, false, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &DiffCaretRight, _, cx| {
                    this.move_caret(1, 0, None, false, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &DiffSelectLeft, _, cx| {
                    this.move_caret(-1, 0, None, true, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &DiffSelectRight, _, cx| {
                    this.move_caret(1, 0, None, true, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &DiffSelectUp, _, cx| this.move_caret(0, -1, None, true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &DiffSelectDown, _, cx| {
                    this.move_caret(0, 1, None, true, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &DiffSelectStart, _, cx| {
                this.move_caret(0, 0, Some(false), true, cx)
            }))
            .on_action(cx.listener(|this, _: &DiffSelectEnd, _, cx| {
                this.move_caret(0, 0, Some(true), true, cx)
            }))
            .on_action(cx.listener(|this, _: &DiffSelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &ScrollUp, _, cx| this.scroll_rows(false, 1, cx)))
            .on_action(cx.listener(|this, _: &ScrollDown, _, cx| this.scroll_rows(true, 1, cx)))
            .on_action(
                cx.listener(|this, _: &PageUp, _, cx| {
                    this.scroll_rows(false, this.page_rows(), cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _, cx| {
                    this.scroll_rows(true, this.page_rows(), cx)
                }),
            )
            .on_action(cx.listener(|this, _: &HalfPageUp, _, cx| {
                this.scroll_rows(false, (this.page_rows() / 2).max(1), cx)
            }))
            .on_action(cx.listener(|this, _: &HalfPageDown, _, cx| {
                this.scroll_rows(true, (this.page_rows() / 2).max(1), cx)
            }))
            .on_action(
                cx.listener(|this, _: &CursorFirst, _, cx| this.scroll_rows(false, usize::MAX, cx)),
            )
            .on_action(
                cx.listener(|this, _: &CursorLast, _, cx| this.scroll_rows(true, usize::MAX, cx)),
            )
            .on_action(cx.listener(|this, _: &ExpandContext, _, cx| this.expand_visible(cx)))
            .on_action(cx.listener(|this, _: &FoldContext, _, cx| this.fold_visible(cx)))
            .on_action(cx.listener(|this, _: &ToggleFolds, _, cx| this.toggle_folds(cx)))
            .on_action(cx.listener(|this, _: &NextHunk, _, cx| this.hunk(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousHunk, _, cx| this.hunk(false, cx)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .p_2()
                    .child(
                        Button::new("previous-hunk")
                            .label("Previous hunk")
                            .on_click(cx.listener(|this, _, _, cx| this.hunk(false, cx))),
                    )
                    .child(
                        Button::new("next-hunk")
                            .label("Next hunk")
                            .on_click(cx.listener(|this, _, _, cx| this.hunk(true, cx))),
                    )
                    .child(
                        Button::new("collapse-context")
                            .label("Fold context")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.expanded.clear();
                                this.rebuild(cx);
                            })),
                    )
                    .child(
                        Button::new("copy-diff")
                            .label("Copy diff / selection")
                            .on_click(
                                cx.listener(|this, _, w, cx| this.copy(&CopySelection, w, cx)),
                            ),
                    ),
            )
            .child(div().p_2().child(match self.state.direction {
                Decision::Upload => "Remote → Local (Upload preview)",
                Decision::Download | Decision::Skip => "Local → Remote (Download preview)",
                Decision::DeleteLocal => "Local → removed",
                Decision::DeleteRemote => "Remote → removed",
            }))
            .child(if self.rows.is_empty() {
                div()
                    .p_3()
                    .child(if self.result.as_ref().is_some_and(|r| r.binary) {
                        "Binary or large file: content comparison only".into()
                    } else {
                        self.message.clone()
                    })
                    .into_any_element()
            } else {
                let callbacks = entity.clone();
                let viewport_geometry = geometry.clone();
                div()
                    .id("diff-viewport")
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_hidden()
                    .child(viewport::Viewport::new(
                        uniform_list(
                            "diff-lines",
                            self.rows.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                let span = this.result.as_ref().and_then(|result| {
                                    this.state
                                        .selection
                                        .and_then(|selection| selection.span(result, &this.rows))
                                });
                                range
                                    .map(|i| {
                                        let row = &this.rows[i];
                                        let color = match row {
                                            Row::Line(index) => match hunks::line(
                                                &this.result.as_ref().unwrap().lines[*index],
                                                this.state.direction,
                                            )
                                            .2
                                            {
                                                LineKind::Added => cx.theme().success,
                                                LineKind::Removed => cx.theme().danger,
                                                LineKind::Equal => cx.theme().foreground,
                                            },
                                            _ => cx.theme().muted_foreground,
                                        };
                                        let element = div()
                                            .id(("diff-row", i))
                                            .h(px(ROW_HEIGHT))
                                            .px_2()
                                            .flex()
                                            .font_family("monospace")
                                            .whitespace_nowrap()
                                            .text_color(color);
                                        let element = match row {
                                            Row::Line(source) => {
                                                let result = this.result.as_ref().unwrap();
                                                let body = &result.lines[*source].text;
                                                let formatted = this.row_text(row);
                                                let gutter = formatted
                                                    [..formatted.len() - body.len()]
                                                    .to_owned();
                                                let range = span.as_ref().and_then(|span| {
                                                    span.range_for(i, *source, body.len())
                                                });
                                                let cursor = this
                                                    .state
                                                    .selection
                                                    .filter(|selection| {
                                                        caret
                                                            && selection.collapsed()
                                                            && selection.head.source == *source
                                                    })
                                                    .map(|selection| selection.head.byte);
                                                element.child(gutter).child(text::TextRun::new(
                                                    *source,
                                                    body.clone().into(),
                                                    range,
                                                    cursor,
                                                    cx.theme().selection,
                                                    geometry.clone(),
                                                ))
                                            }
                                            Row::Header { .. } => element.child(this.row_text(row)),
                                            Row::Fold { .. } => element
                                                .child(this.row_text(row))
                                                .on_click(cx.listener(
                                                move |this, event: &gpui_kit::ClickEvent, w, cx| {
                                                    if this.revision == revision {
                                                        this.select(
                                                            i,
                                                            event.modifiers().shift,
                                                            w,
                                                            cx,
                                                        );
                                                    }
                                                },
                                            )),
                                        };
                                        #[cfg(test)]
                                        let element = element.test_support();
                                        element
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.state.scroll)
                        .flex_1()
                        .min_h_0(),
                        viewport_geometry,
                        callbacks,
                        revision,
                    ))
                    .into_any_element()
            })
    }
}
